// ABOUTME: Wayland compositor for MobileOS, built on smithay.
// ABOUTME: Handles display output, window management, and touch input.

mod handlers;
mod input;
mod state;
mod udev;
mod winit;

use smithay::reexports::calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
use tracing::info;

use crate::state::Compositor;

enum Backend {
    Winit,
    Udev,
}

fn select_backend() -> Backend {
    if std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_ok() {
        Backend::Winit
    } else {
        Backend::Udev
    }
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .compact()
        .init();

    info!("MobileOS compositor starting");

    let mut event_loop: EventLoop<Compositor> = EventLoop::try_new()?;
    let display: Display<Compositor> = Display::new()?;
    let mut state = Compositor::new(&mut event_loop, display);

    info!(socket = ?state.socket_name, "wayland socket ready");

    match select_backend() {
        Backend::Winit => {
            info!("using winit backend (desktop development)");
            winit::init_winit(&mut event_loop, &mut state)?;
        }
        Backend::Udev => {
            info!("using udev/DRM backend (hardware)");
            udev::init_udev(&mut event_loop, &mut state)?;
        }
    }

    // SAFETY: called before spawning any threads, single-threaded at this point
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &state.socket_name) };

    info!("entering event loop");
    event_loop.run(None, &mut state, |_| {})?;

    Ok(())
}
