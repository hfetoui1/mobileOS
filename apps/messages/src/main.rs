// ABOUTME: SMS messaging application for MobileOS.
// ABOUTME: Connects to org.mobileos.Modem via D-Bus for sending messages.

use std::sync::mpsc;

use tracing::info;

slint::include_modules!();

struct SmsCommand {
    number: String,
    message: String,
}

#[zbus::proxy(
    interface = "org.mobileos.Modem",
    default_service = "org.mobileos.Modem",
    default_path = "/org/mobileos/Modem"
)]
trait Modem {
    fn send_sms(&self, number: &str, message: &str) -> zbus::Result<()>;
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("starting messages");

    let window = MessagesWindow::new()?;
    let (sms_tx, sms_rx) = mpsc::channel::<SmsCommand>();

    window.on_send_message(move |contact, text| {
        let contact = contact.to_string();
        let text = text.to_string();
        if !text.is_empty() {
            info!(to = %contact, "sending message");
            let _ = sms_tx.send(SmsCommand {
                number: contact,
                message: text,
            });
        }
    });

    // Background tokio thread for D-Bus communication
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

            while let Ok(cmd) = sms_rx.recv() {
                if let Err(e) = proxy.send_sms(&cmd.number, &cmd.message).await {
                    info!("send_sms failed: {e}");
                }
            }
        });
    });

    info!("messages running");
    window.run()?;

    Ok(())
}
