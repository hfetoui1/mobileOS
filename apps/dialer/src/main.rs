// ABOUTME: Phone dialer application for MobileOS.
// ABOUTME: Connects to org.mobileos.Modem via D-Bus for call management.

use std::sync::mpsc;

use tracing::info;

slint::include_modules!();

enum ModemCommand {
    Dial(String),
    HangUp,
}

#[zbus::proxy(
    interface = "org.mobileos.Modem",
    default_service = "org.mobileos.Modem",
    default_path = "/org/mobileos/Modem"
)]
trait Modem {
    fn dial(&self, number: &str) -> zbus::Result<()>;
    fn hang_up(&self) -> zbus::Result<()>;

    #[zbus(property)]
    fn modem_state(&self) -> zbus::Result<String>;
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("starting dialer");

    let window = DialerWindow::new()?;
    let (cmd_tx, cmd_rx) = mpsc::channel::<ModemCommand>();

    // Digit pressed — append to phone number
    let weak = window.as_weak();
    window.on_digit_pressed(move |digit| {
        if let Some(w) = weak.upgrade() {
            let current = w.get_phone_number();
            w.set_phone_number(format!("{current}{digit}").into());
        }
    });

    // Call pressed
    let tx = cmd_tx.clone();
    let weak = window.as_weak();
    window.on_call_pressed(move || {
        if let Some(w) = weak.upgrade() {
            let number = w.get_phone_number().to_string();
            if !number.is_empty() {
                let _ = tx.send(ModemCommand::Dial(number));
                w.set_call_status("dialing...".into());
            }
        }
    });

    // Hangup pressed
    let tx = cmd_tx;
    let weak = window.as_weak();
    window.on_hangup_pressed(move || {
        if let Some(w) = weak.upgrade() {
            let _ = tx.send(ModemCommand::HangUp);
            w.set_call_status("idle".into());
        }
    });

    // Background tokio thread for D-Bus communication
    let weak = window.as_weak();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let conn = match zbus::Connection::session().await {
                Ok(c) => c,
                Err(e) => {
                    info!("D-Bus not available: {e}");
                    return;
                }
            };

            let proxy = match ModemProxy::new(&conn).await {
                Ok(p) => p,
                Err(e) => {
                    info!("modem service not available: {e}");
                    return;
                }
            };

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    ModemCommand::Dial(number) => {
                        info!(number = %number, "dialing");
                        if let Err(e) = proxy.dial(&number).await {
                            info!("dial failed: {e}");
                        }
                        if let Ok(state) = proxy.modem_state().await {
                            let weak = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(w) = weak.upgrade() {
                                    w.set_call_status(state.into());
                                }
                            });
                        }
                    }
                    ModemCommand::HangUp => {
                        info!("hanging up");
                        if let Err(e) = proxy.hang_up().await {
                            info!("hang_up failed: {e}");
                        }
                        let weak = weak.clone();
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(w) = weak.upgrade() {
                                w.set_call_status("idle".into());
                            }
                        });
                    }
                }
            }
        });
    });

    info!("dialer running");
    window.run()?;

    Ok(())
}
