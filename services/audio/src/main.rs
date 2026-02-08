// ABOUTME: Audio routing D-Bus daemon for MobileOS.
// ABOUTME: Exposes volume, mute state, and audio profile over org.mobileos.Audio.

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use tracing::info;
use zbus::{connection, interface};

struct AudioService {
    volume: Arc<AtomicU8>,
    muted: Arc<AtomicBool>,
    active_profile: Arc<Mutex<String>>,
}

impl AudioService {
    fn new() -> Self {
        Self {
            volume: Arc::new(AtomicU8::new(50)),
            muted: Arc::new(AtomicBool::new(false)),
            active_profile: Arc::new(Mutex::new("speaker".to_string())),
        }
    }
}

#[interface(name = "org.mobileos.Audio")]
impl AudioService {
    #[zbus(property)]
    fn volume(&self) -> u8 {
        self.volume.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn set_volume(&mut self, value: u8) {
        info!(volume = value, "setting volume");
        self.volume.store(value, Ordering::Relaxed);
    }

    #[zbus(property)]
    fn muted(&self) -> bool {
        self.muted.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn set_muted(&mut self, value: bool) {
        info!(muted = value, "setting mute state");
        self.muted.store(value, Ordering::Relaxed);
    }

    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.active_profile.lock().unwrap().clone()
    }

    #[zbus(property)]
    fn set_active_profile(&mut self, profile: String) {
        info!(profile = %profile, "setting audio profile");
        *self.active_profile.lock().unwrap() = profile;
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

    info!("starting audio service");

    let service = AudioService::new();

    let _connection = connection::Builder::session()?
        .name("org.mobileos.Audio")?
        .serve_at("/org/mobileos/Audio", service)?
        .build()
        .await?;

    info!("audio service running on session bus");

    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use zbus::{connection, proxy, Connection};

    #[proxy(
        interface = "org.mobileos.Audio",
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

        #[zbus(property)]
        fn active_profile(&self) -> zbus::Result<String>;

        #[zbus(property)]
        fn set_active_profile(&self, value: &str) -> zbus::Result<()>;
    }

    async fn start_test_service() -> (Connection, zbus::names::OwnedUniqueName) {
        let service = super::AudioService::new();
        let conn = connection::Builder::session()
            .unwrap()
            .serve_at("/org/mobileos/Audio", service)
            .unwrap()
            .build()
            .await
            .unwrap();
        let name = conn.unique_name().unwrap().to_owned();
        (conn, name)
    }

    #[tokio::test]
    async fn reads_default_volume() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = AudioProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert_eq!(proxy.volume().await.unwrap(), 50);
    }

    #[tokio::test]
    async fn set_volume_updates_property() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = AudioProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        proxy.set_volume(80).await.unwrap();
        assert_eq!(proxy.volume().await.unwrap(), 80);
    }

    #[tokio::test]
    async fn mute_toggle() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = AudioProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert!(!proxy.muted().await.unwrap());
        proxy.set_muted(true).await.unwrap();
        assert!(proxy.muted().await.unwrap());
    }

    #[tokio::test]
    async fn set_profile() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = AudioProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert_eq!(proxy.active_profile().await.unwrap(), "speaker");
        proxy.set_active_profile("headphones").await.unwrap();
        assert_eq!(proxy.active_profile().await.unwrap(), "headphones");
    }
}
