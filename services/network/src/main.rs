// ABOUTME: Network management D-Bus daemon for MobileOS.
// ABOUTME: Exposes WiFi connection state, scanning, and connect/disconnect over org.mobileos.Network.

use std::sync::{Arc, Mutex};

use tracing::info;
use zbus::{connection, interface};

struct NetworkState {
    connected: bool,
    ssid: String,
    ip_address: String,
    connection_type: String,
}

struct NetworkService {
    state: Arc<Mutex<NetworkState>>,
}

impl NetworkService {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(NetworkState {
                connected: false,
                ssid: String::new(),
                ip_address: String::new(),
                connection_type: "none".to_string(),
            })),
        }
    }
}

#[interface(name = "org.mobileos.Network")]
impl NetworkService {
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.state.lock().unwrap().connected
    }

    #[zbus(property)]
    fn ssid(&self) -> String {
        self.state.lock().unwrap().ssid.clone()
    }

    #[zbus(property)]
    fn ip_address(&self) -> String {
        self.state.lock().unwrap().ip_address.clone()
    }

    #[zbus(property)]
    fn connection_type(&self) -> String {
        self.state.lock().unwrap().connection_type.clone()
    }

    async fn scan(&self) -> Vec<String> {
        info!("scanning for networks");
        vec![
            "HomeWiFi".to_string(),
            "CoffeeShop".to_string(),
            "FreeNet".to_string(),
        ]
    }

    async fn connect(&self, ssid: String, _password: String) {
        info!(ssid = %ssid, "connecting to network");
        let mut state = self.state.lock().unwrap();
        state.connected = true;
        state.ssid = ssid;
        state.ip_address = "192.168.1.100".to_string();
        state.connection_type = "wifi".to_string();
    }

    async fn disconnect(&self) {
        info!("disconnecting from network");
        let mut state = self.state.lock().unwrap();
        state.connected = false;
        state.ssid.clear();
        state.ip_address.clear();
        state.connection_type = "none".to_string();
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

    info!("starting network service");

    let service = NetworkService::new();

    let _connection = connection::Builder::session()?
        .name("org.mobileos.Network")?
        .serve_at("/org/mobileos/Network", service)?
        .build()
        .await?;

    info!("network service running on session bus");

    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use zbus::{connection, proxy, Connection};

    #[proxy(
        interface = "org.mobileos.Network",
        default_path = "/org/mobileos/Network"
    )]
    trait Network {
        #[zbus(property)]
        fn connected(&self) -> zbus::Result<bool>;

        #[zbus(property)]
        fn ssid(&self) -> zbus::Result<String>;

        #[zbus(property)]
        fn ip_address(&self) -> zbus::Result<String>;

        #[zbus(property)]
        fn connection_type(&self) -> zbus::Result<String>;

        fn scan(&self) -> zbus::Result<Vec<String>>;
        fn connect(&self, ssid: &str, password: &str) -> zbus::Result<()>;
        fn disconnect(&self) -> zbus::Result<()>;
    }

    async fn start_test_service() -> (Connection, zbus::names::OwnedUniqueName) {
        let service = super::NetworkService::new();
        let conn = connection::Builder::session()
            .unwrap()
            .serve_at("/org/mobileos/Network", service)
            .unwrap()
            .build()
            .await
            .unwrap();
        let name = conn.unique_name().unwrap().to_owned();
        (conn, name)
    }

    #[tokio::test]
    async fn starts_disconnected() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = NetworkProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert!(!proxy.connected().await.unwrap());
        assert_eq!(proxy.connection_type().await.unwrap(), "none");
    }

    #[tokio::test]
    async fn scan_returns_networks() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = NetworkProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        let networks = proxy.scan().await.unwrap();
        assert_eq!(networks.len(), 3);
        assert!(networks.contains(&"HomeWiFi".to_string()));
    }

    #[tokio::test]
    async fn connect_updates_state() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = NetworkProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        proxy.connect("HomeWiFi", "password123").await.unwrap();
        assert!(proxy.connected().await.unwrap());
        assert_eq!(proxy.ssid().await.unwrap(), "HomeWiFi");
        assert_eq!(proxy.connection_type().await.unwrap(), "wifi");
    }

    #[tokio::test]
    async fn disconnect_clears_state() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = NetworkProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        proxy.connect("HomeWiFi", "pass").await.unwrap();
        proxy.disconnect().await.unwrap();
        assert!(!proxy.connected().await.unwrap());
        assert_eq!(proxy.ssid().await.unwrap(), "");
    }
}
