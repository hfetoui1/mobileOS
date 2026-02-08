// ABOUTME: Input event processing for keyboard, pointer, and touch.
// ABOUTME: Routes backend input events to the appropriate Wayland seat devices.

use smithay::backend::input::{
    AbsolutePositionEvent, Event, InputEvent, KeyboardKeyEvent, PointerButtonEvent, TouchEvent,
};
use smithay::input::keyboard::FilterResult;
use smithay::input::pointer::{ButtonEvent, MotionEvent};
use smithay::input::touch;
use smithay::utils::SERIAL_COUNTER;

use crate::state::Compositor;

impl Compositor {
    pub fn process_input_event<I: smithay::backend::input::InputBackend>(
        &mut self,
        event: InputEvent<I>,
    ) {
        match event {
            InputEvent::Keyboard { event } => self.on_keyboard::<I>(event),
            InputEvent::PointerMotionAbsolute { event } => {
                self.on_pointer_move_absolute::<I>(event)
            }
            InputEvent::PointerButton { event } => self.on_pointer_button::<I>(event),
            InputEvent::TouchDown { event } => self.on_touch_down::<I>(event),
            InputEvent::TouchMotion { event } => self.on_touch_motion::<I>(event),
            InputEvent::TouchUp { event } => self.on_touch_up::<I>(event),
            InputEvent::TouchFrame { .. } => {
                let touch = self.seat.get_touch().unwrap();
                touch.frame(self);
            }
            _ => {}
        }
    }

    fn on_keyboard<I: smithay::backend::input::InputBackend>(
        &mut self,
        event: I::KeyboardKeyEvent,
    ) {
        let serial = SERIAL_COUNTER.next_serial();
        let time = Event::time_msec(&event);
        let keyboard = self.seat.get_keyboard().unwrap();

        keyboard.input::<(), _>(
            self,
            event.key_code(),
            event.state(),
            serial,
            time,
            |_, _, _| FilterResult::Forward,
        );
    }

    fn on_pointer_move_absolute<I: smithay::backend::input::InputBackend>(
        &mut self,
        event: I::PointerMotionAbsoluteEvent,
    ) {
        let output = self.space.outputs().next().cloned();
        let output_geo = output
            .as_ref()
            .map(|o| self.space.output_geometry(o).unwrap());

        if let Some(geo) = output_geo {
            let pos = event.position_transformed(geo.size);
            let serial = SERIAL_COUNTER.next_serial();

            let under = self.space.element_under(pos);
            let surface_under = under.and_then(|(window, loc)| {
                window
                    .surface_under(pos - loc.to_f64(), smithay::desktop::WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, p.to_f64() + loc.to_f64()))
            });

            let pointer = self.seat.get_pointer().unwrap();
            pointer.motion(
                self,
                surface_under,
                &MotionEvent {
                    location: pos,
                    serial,
                    time: event.time_msec(),
                },
            );

            let keyboard = self.seat.get_keyboard().unwrap();
            let focus = self
                .space
                .element_under(pos)
                .and_then(|(w, _)| w.toplevel().map(|t| t.wl_surface().clone()));
            keyboard.set_focus(self, focus, serial);
        }
    }

    fn on_pointer_button<I: smithay::backend::input::InputBackend>(
        &mut self,
        event: I::PointerButtonEvent,
    ) {
        let serial = SERIAL_COUNTER.next_serial();
        let pointer = self.seat.get_pointer().unwrap();

        pointer.button(
            self,
            &ButtonEvent {
                button: event.button_code(),
                state: event.state(),
                serial,
                time: event.time_msec(),
            },
        );
    }

    fn on_touch_down<I: smithay::backend::input::InputBackend>(
        &mut self,
        event: I::TouchDownEvent,
    ) {
        let output = self.space.outputs().next().cloned();
        let output_geo = output
            .as_ref()
            .map(|o| self.space.output_geometry(o).unwrap());

        if let Some(geo) = output_geo {
            let pos = event.position_transformed(geo.size);
            let serial = SERIAL_COUNTER.next_serial();

            let under = self.space.element_under(pos);
            let focus = under.and_then(|(window, loc)| {
                window
                    .surface_under(pos - loc.to_f64(), smithay::desktop::WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, p.to_f64() + loc.to_f64()))
            });

            let touch_handle = self.seat.get_touch().unwrap();
            touch_handle.down(
                self,
                focus,
                &touch::DownEvent {
                    slot: event.slot(),
                    location: pos,
                    serial,
                    time: event.time_msec(),
                },
            );
        }
    }

    fn on_touch_motion<I: smithay::backend::input::InputBackend>(
        &mut self,
        event: I::TouchMotionEvent,
    ) {
        let output = self.space.outputs().next().cloned();
        let output_geo = output
            .as_ref()
            .map(|o| self.space.output_geometry(o).unwrap());

        if let Some(geo) = output_geo {
            let pos = event.position_transformed(geo.size);

            let under = self.space.element_under(pos);
            let focus = under.and_then(|(window, loc)| {
                window
                    .surface_under(pos - loc.to_f64(), smithay::desktop::WindowSurfaceType::ALL)
                    .map(|(s, p)| (s, p.to_f64() + loc.to_f64()))
            });

            let touch_handle = self.seat.get_touch().unwrap();
            touch_handle.motion(
                self,
                focus,
                &touch::MotionEvent {
                    slot: event.slot(),
                    location: pos,
                    time: event.time_msec(),
                },
            );
        }
    }

    fn on_touch_up<I: smithay::backend::input::InputBackend>(
        &mut self,
        event: I::TouchUpEvent,
    ) {
        let serial = SERIAL_COUNTER.next_serial();
        let touch_handle = self.seat.get_touch().unwrap();

        touch_handle.up(
            self,
            &touch::UpEvent {
                slot: event.slot(),
                serial,
                time: event.time_msec(),
            },
        );
    }
}
