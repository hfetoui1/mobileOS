// ABOUTME: DRM/udev backend for real hardware and QEMU virtio-gpu.
// ABOUTME: Opens a libseat session, enumerates DRM devices, and drives the display via GBM/EGL/GLES.

use std::collections::HashSet;
use std::path::Path;

use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::Session;
use smithay::backend::udev::{UdevBackend, UdevEvent};
use smithay::desktop::space::space_render_elements;
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::EventLoop;
use smithay::utils::{DeviceFd, Transform};
use smithay_drm_extras::drm_scanner::{DrmScanEvent, DrmScanner};

use drm_fourcc::{DrmFormat, DrmFourcc};
use rustix::fs::OFlags;
use tracing::{error, info, warn};

use crate::state::Compositor;

const CLEAR_COLOR: [f32; 4] = [0.1, 0.1, 0.1, 1.0];
const COLOR_FORMATS: &[DrmFourcc] = &[DrmFourcc::Argb8888, DrmFourcc::Xrgb8888];

pub fn init_udev(
    event_loop: &mut EventLoop<Compositor>,
    state: &mut Compositor,
) -> anyhow::Result<()> {
    let (mut session, notifier) =
        LibSeatSession::new().map_err(|e| anyhow::anyhow!("failed to open libseat session: {e}"))?;

    let seat_name = session.seat();
    info!(seat = %seat_name, "libseat session opened");

    let udev = UdevBackend::new(&seat_name)
        .map_err(|e| anyhow::anyhow!("failed to create udev backend: {e}"))?;

    for (device_id, path) in udev.device_list() {
        if let Err(e) = add_device(event_loop, state, &mut session, device_id, path) {
            warn!(device_id, ?path, "failed to add device: {e}");
        }
    }

    let handle = event_loop.handle();

    handle
        .insert_source(notifier, |event, _, _state| {
            info!(?event, "seat event");
        })
        .map_err(|e| anyhow::anyhow!("failed to insert seat notifier: {e}"))?;

    handle
        .insert_source(udev, move |event, _, _state| match event {
            UdevEvent::Added { device_id, path } => {
                info!(device_id, ?path, "udev device added");
            }
            UdevEvent::Changed { device_id } => {
                info!(device_id, "udev device changed");
            }
            UdevEvent::Removed { device_id } => {
                info!(device_id, "udev device removed");
            }
        })
        .map_err(|e| anyhow::anyhow!("failed to insert udev source: {e}"))?;

    Ok(())
}

fn add_device(
    event_loop: &mut EventLoop<Compositor>,
    state: &mut Compositor,
    session: &mut LibSeatSession,
    device_id: libc::dev_t,
    path: &Path,
) -> anyhow::Result<()> {
    let fd = session
        .open(path, OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY)
        .map_err(|e| anyhow::anyhow!("failed to open DRM device {}: {e}", path.display()))?;

    let device_fd = DrmDeviceFd::new(DeviceFd::from(fd));

    let (drm_device, drm_notifier) = DrmDevice::new(device_fd.clone(), false)
        .map_err(|e| anyhow::anyhow!("failed to create DRM device: {e}"))?;

    let gbm_device = GbmDevice::new(device_fd.clone())
        .map_err(|e| anyhow::anyhow!("failed to create GBM device: {e}"))?;

    let egl_display = unsafe { EGLDisplay::new(gbm_device.clone()) }
        .map_err(|e| anyhow::anyhow!("failed to create EGL display: {e}"))?;

    let egl_context = EGLContext::new(&egl_display)
        .map_err(|e| anyhow::anyhow!("failed to create EGL context: {e}"))?;

    let renderer = unsafe { GlesRenderer::new(egl_context) }
        .map_err(|e| anyhow::anyhow!("failed to create GLES renderer: {e}"))?;

    info!(device_id, ?path, "DRM device initialized");

    let mut scanner = DrmScanner::new();
    let scan_result = scanner
        .scan_connectors(&drm_device)
        .map_err(|e| anyhow::anyhow!("failed to scan connectors: {e}"))?;

    // Store DRM state before adding connectors (which needs to mutate it)
    state.drm = Some(DrmState {
        device: drm_device,
        gbm_device,
        device_fd,
        renderer,
        scanner,
        drm_compositor: None,
    });

    for event in scan_result {
        match event {
            DrmScanEvent::Connected {
                connector,
                crtc: Some(crtc),
            } => {
                info!(
                    connector = ?connector.interface(),
                    "connector connected"
                );

                if let Err(e) = add_connector(state, &connector, crtc) {
                    warn!(
                        connector = ?connector.interface(),
                        "failed to add connector: {e}"
                    );
                }
            }
            DrmScanEvent::Connected {
                connector,
                crtc: None,
            } => {
                warn!(
                    connector = ?connector.interface(),
                    "connector has no available CRTC"
                );
            }
            DrmScanEvent::Disconnected { connector, .. } => {
                info!(
                    connector = ?connector.interface(),
                    "connector disconnected"
                );
            }
        }
    }

    let handle = event_loop.handle();
    handle
        .insert_source(drm_notifier, move |event, _metadata, state| {
            match event {
                DrmEvent::VBlank(_crtc) => {
                    if let Some(ref mut drm) = state.drm {
                        if let Some(ref mut compositor) = drm.drm_compositor {
                            if let Err(e) = compositor.frame_submitted() {
                                error!("failed to mark frame as submitted: {e}");
                            }
                        }
                    }
                    render_frame(state);
                }
                DrmEvent::Error(e) => {
                    error!("DRM error: {e}");
                }
            }
        })
        .map_err(|e| anyhow::anyhow!("failed to insert DRM notifier: {e}"))?;

    // Schedule initial render
    render_frame(state);

    Ok(())
}

fn add_connector(
    state: &mut Compositor,
    connector: &drm::control::connector::Info,
    crtc: drm::control::crtc::Handle,
) -> anyhow::Result<()> {
    let drm = state.drm.as_mut().ok_or_else(|| anyhow::anyhow!("no DRM state"))?;

    let mode = connector
        .modes()
        .iter()
        .find(|m| m.mode_type().contains(drm::control::ModeTypeFlags::PREFERRED))
        .or_else(|| connector.modes().first())
        .copied()
        .ok_or_else(|| anyhow::anyhow!("no modes available for connector"))?;

    let (w, h) = mode.size();
    info!(width = w, height = h, "selected mode");

    let surface = drm.device
        .create_surface(crtc, mode, &[connector.handle()])
        .map_err(|e| anyhow::anyhow!("failed to create DRM surface: {e}"))?;

    let allocator = GbmAllocator::new(
        drm.gbm_device.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );

    let exporter = GbmFramebufferExporter::new(drm.gbm_device.clone(), None);

    let renderer_formats: HashSet<DrmFormat> = drm.renderer
        .egl_context()
        .dmabuf_render_formats()
        .iter()
        .copied()
        .collect();

    let output = Output::new(
        connector
            .interface()
            .as_str()
            .to_owned(),
        PhysicalProperties {
            size: (w as i32, h as i32).into(),
            subpixel: Subpixel::Unknown,
            make: "MobileOS".into(),
            model: "DRM".into(),
        },
    );

    let output_mode = Mode {
        size: (w as i32, h as i32).into(),
        refresh: (mode.vrefresh() * 1000) as i32,
    };

    let _global = output.create_global::<Compositor>(&state.display_handle);
    output.change_current_state(
        Some(output_mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(output_mode);
    state.space.map_output(&output, (0, 0));

    let drm_compositor = DrmCompositor::new(
        &output,
        surface,
        None,
        allocator,
        exporter,
        COLOR_FORMATS.iter().copied(),
        renderer_formats,
        drm.device.cursor_size(),
        Some(drm.gbm_device.clone()),
    )
    .map_err(|e| anyhow::anyhow!("failed to create DRM compositor: {e}"))?;

    drm.drm_compositor = Some(drm_compositor);

    info!("DRM output configured");
    Ok(())
}

pub fn render_frame(state: &mut Compositor) {
    let output = match state.space.outputs().next().cloned() {
        Some(o) => o,
        None => return,
    };

    let drm = match state.drm.as_mut() {
        Some(d) => d,
        None => return,
    };
    let drm_compositor = match drm.drm_compositor.as_mut() {
        Some(c) => c,
        None => return,
    };

    let elements = match space_render_elements(
        &mut drm.renderer,
        [&state.space],
        &output,
        1.0,
    ) {
        Ok(e) => e,
        Err(e) => {
            warn!("failed to collect render elements: {e}");
            return;
        }
    };

    match drm_compositor.render_frame::<_, _>(
        &mut drm.renderer,
        &elements,
        CLEAR_COLOR,
        FrameFlags::DEFAULT,
    ) {
        Ok(result) => {
            if !result.is_empty {
                if let Err(e) = drm_compositor.queue_frame(()) {
                    error!("failed to queue frame: {e}");
                }
            }
        }
        Err(e) => {
            warn!("failed to render frame: {e}");
        }
    }

    state.space.elements().for_each(|window| {
        window.send_frame(
            &output,
            state.start_time.elapsed(),
            Some(std::time::Duration::ZERO),
            |_, _| Some(output.clone()),
        );
    });

    state.space.refresh();
    state.popups.cleanup();
    let _ = state.display_handle.flush_clients();
}

pub struct DrmState {
    pub device: DrmDevice,
    pub gbm_device: GbmDevice<DrmDeviceFd>,
    pub device_fd: DrmDeviceFd,
    pub renderer: GlesRenderer,
    pub scanner: DrmScanner,
    pub drm_compositor:
        Option<DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_formats_not_empty() {
        assert!(!COLOR_FORMATS.is_empty());
    }

    #[test]
    fn clear_color_is_valid() {
        for &c in &CLEAR_COLOR {
            assert!((0.0..=1.0).contains(&c));
        }
    }
}
