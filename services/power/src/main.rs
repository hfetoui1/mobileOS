// ABOUTME: Power management D-Bus daemon for MobileOS.
// ABOUTME: Exposes battery level, charging state, and screen brightness over org.mobileos.Power.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

use tracing::info;
use zbus::{connection, interface};

struct PowerService {
    battery_level: Arc<AtomicU8>,
    charging: Arc<AtomicBool>,
    brightness: Arc<AtomicU8>,
}

impl PowerService {
    fn new() -> Self {
        Self {
            battery_level: Arc::new(AtomicU8::new(85)),
            charging: Arc::new(AtomicBool::new(false)),
            brightness: Arc::new(AtomicU8::new(128)),
        }
    }
}

#[interface(name = "org.mobileos.Power")]
impl PowerService {
    #[zbus(property)]
    fn battery_level(&self) -> u8 {
        self.battery_level.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn charging(&self) -> bool {
        self.charging.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn screen_brightness(&self) -> u8 {
        self.brightness.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn set_screen_brightness(&mut self, value: u8) {
        info!(brightness = value, "setting screen brightness");
        self.brightness.store(value, Ordering::Relaxed);
    }

    async fn suspend(&self) {
        info!("suspend requested");
        #[cfg(feature = "hardware")]
        {
            // Write to /sys/power/state
        }
    }

    async fn shutdown(&self) {
        info!("shutdown requested");
        #[cfg(feature = "hardware")]
        {
            // Trigger system shutdown
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("starting power service");

    let service = PowerService::new();

    let _connection = connection::Builder::session()?
        .name("org.mobileos.Power")?
        .serve_at("/org/mobileos/Power", service)?
        .build()
        .await?;

    info!("power service running on session bus");

    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use zbus::{connection, proxy, Connection};

    #[proxy(
        interface = "org.mobileos.Power",
        default_path = "/org/mobileos/Power"
    )]
    trait Power {
        #[zbus(property)]
        fn battery_level(&self) -> zbus::Result<u8>;

        #[zbus(property)]
        fn charging(&self) -> zbus::Result<bool>;

        #[zbus(property)]
        fn screen_brightness(&self) -> zbus::Result<u8>;

        #[zbus(property)]
        fn set_screen_brightness(&self, value: u8) -> zbus::Result<()>;

        fn suspend(&self) -> zbus::Result<()>;
        fn shutdown(&self) -> zbus::Result<()>;
    }

    async fn start_test_service() -> (Connection, zbus::names::OwnedUniqueName) {
        let service = super::PowerService::new();
        let conn = connection::Builder::session()
            .unwrap()
            .serve_at("/org/mobileos/Power", service)
            .unwrap()
            .build()
            .await
            .unwrap();
        let name = conn.unique_name().unwrap().to_owned();
        (conn, name)
    }

    #[tokio::test]
    async fn reads_default_battery_level() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = PowerProxy::builder(&client)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();

        assert_eq!(proxy.battery_level().await.unwrap(), 85);
    }

    #[tokio::test]
    async fn reads_default_charging_state() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = PowerProxy::builder(&client)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();

        assert!(!proxy.charging().await.unwrap());
    }

    #[tokio::test]
    async fn set_brightness_updates_property() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = PowerProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert_eq!(proxy.screen_brightness().await.unwrap(), 128);
        proxy.set_screen_brightness(200).await.unwrap();
        assert_eq!(proxy.screen_brightness().await.unwrap(), 200);
    }

    #[tokio::test]
    async fn suspend_does_not_error() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = PowerProxy::builder(&client)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();

        proxy.suspend().await.unwrap();
    }

    #[tokio::test]
    async fn shutdown_does_not_error() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = PowerProxy::builder(&client)
            .destination(name)
            .unwrap()
            .build()
            .await
            .unwrap();

        proxy.shutdown().await.unwrap();
    }
}
