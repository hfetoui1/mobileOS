// ABOUTME: Central compositor state holding all Wayland protocol states.
// ABOUTME: Manages the Display, seat, space, and protocol globals lifecycle.

use std::ffi::OsString;
use std::sync::Arc;

use smithay::desktop::{PopupManager, Space, Window};
use smithay::input::{Seat, SeatState};
use smithay::reexports::calloop::generic::Generic;
use smithay::reexports::calloop::{EventLoop, Interest, LoopSignal, Mode, PostAction};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::{Display, DisplayHandle};
use smithay::wayland::compositor::{CompositorClientState, CompositorState};
use smithay::wayland::output::OutputManagerState;
use smithay::wayland::selection::data_device::DataDeviceState;
use smithay::wayland::shell::xdg::XdgShellState;
use smithay::wayland::shm::ShmState;
use smithay::wayland::socket::ListeningSocketSource;
use tracing::info;

pub struct Compositor {
    pub start_time: std::time::Instant,
    pub socket_name: OsString,
    pub display_handle: DisplayHandle,

    pub space: Space<Window>,
    pub loop_signal: LoopSignal,

    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShellState,
    pub shm_state: ShmState,
    pub output_manager_state: OutputManagerState,
    pub seat_state: SeatState<Compositor>,
    pub data_device_state: DataDeviceState,
    pub popups: PopupManager,

    pub seat: Seat<Compositor>,
}

#[derive(Default)]
pub struct ClientState {
    pub compositor_state: CompositorClientState,
}

impl ClientData for ClientState {
    fn initialized(&self, _client_id: ClientId) {}
    fn disconnected(&self, _client_id: ClientId, _reason: DisconnectReason) {}
}

impl Compositor {
    pub fn new(event_loop: &mut EventLoop<Self>, display: Display<Self>) -> Self {
        let dh = display.handle();

        let compositor_state = CompositorState::new::<Self>(&dh);
        let xdg_shell_state = XdgShellState::new::<Self>(&dh);
        let shm_state = ShmState::new::<Self>(&dh, vec![]);
        let output_manager_state = OutputManagerState::new_with_xdg_output::<Self>(&dh);
        let data_device_state = DataDeviceState::new::<Self>(&dh);
        let popups = PopupManager::default();

        let mut seat_state = SeatState::new();
        let mut seat: Seat<Self> = seat_state.new_wl_seat(&dh, "seat0");
        seat.add_keyboard(Default::default(), 200, 25)
            .expect("failed to add keyboard to seat");
        seat.add_pointer();

        let space = Space::default();
        let socket_name = Self::init_wayland_listener(display, event_loop);
        let loop_signal = event_loop.get_signal();

        info!(socket = ?socket_name, "compositor initialized");

        Self {
            start_time: std::time::Instant::now(),
            socket_name,
            display_handle: dh,
            space,
            loop_signal,
            compositor_state,
            xdg_shell_state,
            shm_state,
            output_manager_state,
            seat_state,
            data_device_state,
            popups,
            seat,
        }
    }

    fn init_wayland_listener(
        display: Display<Self>,
        event_loop: &mut EventLoop<Self>,
    ) -> OsString {
        let listening_socket = ListeningSocketSource::new_auto()
            .expect("failed to create wayland listening socket");
        let socket_name = listening_socket.socket_name().to_os_string();
        let handle = event_loop.handle();

        handle
            .insert_source(listening_socket, move |client_stream, _, state| {
                state
                    .display_handle
                    .insert_client(client_stream, Arc::new(ClientState::default()))
                    .unwrap();
            })
            .expect("failed to insert wayland listener source");

        handle
            .insert_source(
                Generic::new(display, Interest::READ, Mode::Level),
                |_, display, state| {
                    unsafe {
                        display.get_mut().dispatch_clients(state).unwrap();
                    }
                    Ok(PostAction::Continue)
                },
            )
            .expect("failed to insert wayland display source");

        socket_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compositor_state_initializes() {
        let mut event_loop: EventLoop<Compositor> = EventLoop::try_new().unwrap();
        let display: Display<Compositor> = Display::new().unwrap();
        let state = Compositor::new(&mut event_loop, display);

        assert!(!state.socket_name.is_empty());
    }

    #[test]
    fn seat_has_keyboard_and_pointer() {
        let mut event_loop: EventLoop<Compositor> = EventLoop::try_new().unwrap();
        let display: Display<Compositor> = Display::new().unwrap();
        let state = Compositor::new(&mut event_loop, display);

        assert!(state.seat.get_keyboard().is_some());
        assert!(state.seat.get_pointer().is_some());
    }

    #[test]
    fn space_starts_empty() {
        let mut event_loop: EventLoop<Compositor> = EventLoop::try_new().unwrap();
        let display: Display<Compositor> = Display::new().unwrap();
        let state = Compositor::new(&mut event_loop, display);

        assert_eq!(state.space.elements().count(), 0);
    }
}
