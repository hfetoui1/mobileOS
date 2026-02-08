// ABOUTME: System settings application for MobileOS.
// ABOUTME: Connects to Power, Network, and Audio services via D-Bus.

use std::sync::mpsc;

use tracing::info;

slint::include_modules!();

enum SettingsCommand {
    WifiScan,
    WifiConnect(String),
    WifiDisconnect,
    SetBrightness(u8),
    SetVolume(u8),
    SetMuted(bool),
}

#[zbus::proxy(
    interface = "org.mobileos.Power",
    default_service = "org.mobileos.Power",
    default_path = "/org/mobileos/Power"
)]
trait Power {
    #[zbus(property)]
    fn battery_level(&self) -> zbus::Result<u8>;

    #[zbus(property)]
    fn screen_brightness(&self) -> zbus::Result<u8>;

    #[zbus(property)]
    fn set_screen_brightness(&self, value: u8) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.mobileos.Network",
    default_service = "org.mobileos.Network",
    default_path = "/org/mobileos/Network"
)]
trait Network {
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<String>;

    fn scan(&self) -> zbus::Result<Vec<String>>;
    fn connect(&self, ssid: &str, password: &str) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.mobileos.Audio",
    default_service = "org.mobileos.Audio",
    default_path = "/org/mobileos/Audio"
)]
trait Audio {
    #[zbus(property)]
    fn volume(&self) -> zbus::Result<u8>;

    #[zbus(property)]
    fn set_volume(&self, value: u8) -> zbus::Result<()>;

    #[zbus(property)]
    fn muted(&self) -> zbus::Result<bool>;

    #[zbus(property)]
    fn set_muted(&self, value: bool) -> zbus::Result<()>;
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("starting settings");

    let window = SettingsWindow::new()?;
    let (cmd_tx, cmd_rx) = mpsc::channel::<SettingsCommand>();

    let tx = cmd_tx.clone();
    window.on_wifi_scan(move || {
        let _ = tx.send(SettingsCommand::WifiScan);
    });

    let tx = cmd_tx.clone();
    window.on_wifi_connect(move |ssid| {
        let _ = tx.send(SettingsCommand::WifiConnect(ssid.to_string()));
    });

    let tx = cmd_tx.clone();
    window.on_wifi_disconnect(move || {
        let _ = tx.send(SettingsCommand::WifiDisconnect);
    });

    let tx = cmd_tx.clone();
    window.on_brightness_changed(move |val| {
        let _ = tx.send(SettingsCommand::SetBrightness(val as u8));
    });

    let tx = cmd_tx.clone();
    window.on_volume_changed(move |val| {
        let _ = tx.send(SettingsCommand::SetVolume(val as u8));
    });

    let tx = cmd_tx;
    window.on_mute_toggled(move |muted| {
        let _ = tx.send(SettingsCommand::SetMuted(muted));
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

            let power = PowerProxy::new(&conn).await.ok();
            let network = NetworkProxy::new(&conn).await.ok();
            let audio = AudioProxy::new(&conn).await.ok();

            // Load initial state
            if let Some(ref p) = power {
                if let Ok(level) = p.battery_level().await {
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            w.set_battery_level(level as i32);
                        }
                    });
                }
                if let Ok(brightness) = p.screen_brightness().await {
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            w.set_brightness(brightness as i32);
                        }
                    });
                }
            }

            if let Some(ref a) = audio {
                if let Ok(vol) = a.volume().await {
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            w.set_volume(vol as i32);
                        }
                    });
                }
                if let Ok(muted) = a.muted().await {
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            w.set_muted(muted);
                        }
                    });
                }
            }

            if let Some(ref n) = network {
                if let Ok(connected) = n.connected().await {
                    let ssid = n.ssid().await.unwrap_or_default();
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(w) = weak.upgrade() {
                            w.set_wifi_connected(connected);
                            w.set_wifi_ssid(ssid.into());
                        }
                    });
                }
            }

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    SettingsCommand::WifiScan => {
                        if let Some(ref n) = network {
                            if let Ok(networks) = n.scan().await {
                                let weak = weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(w) = weak.upgrade() {
                                        let entries: Vec<NetworkEntry> = networks
                                            .into_iter()
                                            .map(|name| NetworkEntry { name: name.into() })
                                            .collect();
                                        let model = std::rc::Rc::new(slint::VecModel::from(entries));
                                        w.set_wifi_networks(model.into());
                                    }
                                });
                            }
                        }
                    }
                    SettingsCommand::WifiConnect(ssid) => {
                        if let Some(ref n) = network {
                            let _ = n.connect(&ssid, "").await;
                            let connected = n.connected().await.unwrap_or(false);
                            let current_ssid = n.ssid().await.unwrap_or_default();
                            let weak = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(w) = weak.upgrade() {
                                    w.set_wifi_connected(connected);
                                    w.set_wifi_ssid(current_ssid.into());
                                }
                            });
                        }
                    }
                    SettingsCommand::WifiDisconnect => {
                        if let Some(ref n) = network {
                            let _ = n.disconnect().await;
                            let weak = weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(w) = weak.upgrade() {
                                    w.set_wifi_connected(false);
                                    w.set_wifi_ssid("".into());
                                }
                            });
                        }
                    }
                    SettingsCommand::SetBrightness(val) => {
                        if let Some(ref p) = power {
                            let _ = p.set_screen_brightness(val).await;
                        }
                    }
                    SettingsCommand::SetVolume(val) => {
                        if let Some(ref a) = audio {
                            let _ = a.set_volume(val).await;
                        }
                    }
                    SettingsCommand::SetMuted(muted) => {
                        if let Some(ref a) = audio {
                            let _ = a.set_muted(muted).await;
                        }
                    }
                }
            }
        });
    });

    info!("settings running");
    window.run()?;

    Ok(())
}
