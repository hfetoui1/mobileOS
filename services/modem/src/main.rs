// ABOUTME: Modem management D-Bus daemon for MobileOS.
// ABOUTME: Exposes signal strength, call state, and SMS over org.mobileos.Modem.

use std::sync::{Arc, Mutex};

use tracing::info;
use zbus::{connection, interface};

struct ModemState {
    signal_strength: u8,
    operator: String,
    sim_present: bool,
    modem_state: String,
}

struct ModemService {
    state: Arc<Mutex<ModemState>>,
}

impl ModemService {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(ModemState {
                signal_strength: 75,
                operator: "MobileOS Carrier".to_string(),
                sim_present: true,
                modem_state: "idle".to_string(),
            })),
        }
    }
}

#[interface(name = "org.mobileos.Modem")]
impl ModemService {
    #[zbus(property)]
    fn signal_strength(&self) -> u8 {
        self.state.lock().unwrap().signal_strength
    }

    #[zbus(property)]
    fn operator(&self) -> String {
        self.state.lock().unwrap().operator.clone()
    }

    #[zbus(property)]
    fn sim_present(&self) -> bool {
        self.state.lock().unwrap().sim_present
    }

    #[zbus(property)]
    fn modem_state(&self) -> String {
        self.state.lock().unwrap().modem_state.clone()
    }

    async fn dial(&self, number: String) {
        info!(number = %number, "dialing");
        self.state.lock().unwrap().modem_state = "in-call".to_string();
    }

    async fn hang_up(&self) {
        info!("hanging up");
        self.state.lock().unwrap().modem_state = "idle".to_string();
    }

    async fn send_sms(&self, number: String, message: String) {
        info!(number = %number, len = message.len(), "sending SMS");
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

    info!("starting modem service");

    let service = ModemService::new();

    let _connection = connection::Builder::session()?
        .name("org.mobileos.Modem")?
        .serve_at("/org/mobileos/Modem", service)?
        .build()
        .await?;

    info!("modem service running on session bus");

    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use zbus::{connection, proxy, Connection};

    #[proxy(
        interface = "org.mobileos.Modem",
        default_path = "/org/mobileos/Modem"
    )]
    trait Modem {
        #[zbus(property)]
        fn signal_strength(&self) -> zbus::Result<u8>;

        #[zbus(property)]
        fn operator(&self) -> zbus::Result<String>;

        #[zbus(property)]
        fn sim_present(&self) -> zbus::Result<bool>;

        #[zbus(property)]
        fn modem_state(&self) -> zbus::Result<String>;

        fn dial(&self, number: &str) -> zbus::Result<()>;
        fn hang_up(&self) -> zbus::Result<()>;
        fn send_sms(&self, number: &str, message: &str) -> zbus::Result<()>;
    }

    async fn start_test_service() -> (Connection, zbus::names::OwnedUniqueName) {
        let service = super::ModemService::new();
        let conn = connection::Builder::session()
            .unwrap()
            .serve_at("/org/mobileos/Modem", service)
            .unwrap()
            .build()
            .await
            .unwrap();
        let name = conn.unique_name().unwrap().to_owned();
        (conn, name)
    }

    #[tokio::test]
    async fn reads_default_signal() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = ModemProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert_eq!(proxy.signal_strength().await.unwrap(), 75);
    }

    #[tokio::test]
    async fn reads_default_operator() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = ModemProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert_eq!(proxy.operator().await.unwrap(), "MobileOS Carrier");
        assert!(proxy.sim_present().await.unwrap());
    }

    #[tokio::test]
    async fn dial_sets_in_call_state() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = ModemProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert_eq!(proxy.modem_state().await.unwrap(), "idle");
        proxy.dial("+1234567890").await.unwrap();
        assert_eq!(proxy.modem_state().await.unwrap(), "in-call");
    }

    #[tokio::test]
    async fn hang_up_returns_to_idle() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = ModemProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        proxy.dial("+1234567890").await.unwrap();
        proxy.hang_up().await.unwrap();
        assert_eq!(proxy.modem_state().await.unwrap(), "idle");
    }

    #[tokio::test]
    async fn send_sms_does_not_error() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = ModemProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        proxy.send_sms("+1234567890", "Hello!").await.unwrap();
    }
}
