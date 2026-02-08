// ABOUTME: MobileOS UI shell — home screen, lock screen, and status bar.
// ABOUTME: Runs as a Wayland client connecting to the MobileOS compositor.

use std::time::Duration;

use slint::TimerMode;
use tracing::info;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("starting shell");

    let window = ShellWindow::new()?;

    window.set_battery("85%".into());
    window.set_network("WiFi".into());
    update_clock(&window);

    let timer = slint::Timer::default();
    let weak = window.as_weak();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        if let Some(w) = weak.upgrade() {
            update_clock(&w);
        }
    });

    window.on_app_launched(|name| {
        info!(app = name.as_str(), "app launched");
    });

    info!("shell running");
    window.run()
}

fn update_clock(window: &ShellWindow) {
    let now = chrono::Local::now();
    window.set_time(now.format("%H:%M").to_string().into());
    window.set_date(now.format("%A, %B %-d").to_string().into());
}
