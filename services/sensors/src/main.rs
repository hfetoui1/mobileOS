// ABOUTME: Sensor D-Bus daemon for MobileOS.
// ABOUTME: Exposes proximity, ambient light, and accelerometer readings over org.mobileos.Sensors.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use tracing::info;
use zbus::{connection, interface};

struct SensorsService {
    proximity: Arc<AtomicBool>,
    ambient_light: Arc<AtomicU32>,
    accel_x: Arc<AtomicU64>,
    accel_y: Arc<AtomicU64>,
    accel_z: Arc<AtomicU64>,
}

impl SensorsService {
    fn new() -> Self {
        Self {
            proximity: Arc::new(AtomicBool::new(false)),
            ambient_light: Arc::new(AtomicU32::new(500)),
            accel_x: Arc::new(AtomicU64::new(0.0f64.to_bits())),
            accel_y: Arc::new(AtomicU64::new(0.0f64.to_bits())),
            accel_z: Arc::new(AtomicU64::new(9.8f64.to_bits())),
        }
    }
}

#[interface(name = "org.mobileos.Sensors")]
impl SensorsService {
    #[zbus(property)]
    fn proximity(&self) -> bool {
        self.proximity.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn ambient_light(&self) -> u32 {
        self.ambient_light.load(Ordering::Relaxed)
    }

    #[zbus(property)]
    fn accelerometer_x(&self) -> f64 {
        f64::from_bits(self.accel_x.load(Ordering::Relaxed))
    }

    #[zbus(property)]
    fn accelerometer_y(&self) -> f64 {
        f64::from_bits(self.accel_y.load(Ordering::Relaxed))
    }

    #[zbus(property)]
    fn accelerometer_z(&self) -> f64 {
        f64::from_bits(self.accel_z.load(Ordering::Relaxed))
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

    info!("starting sensors service");

    let service = SensorsService::new();

    let _connection = connection::Builder::session()?
        .name("org.mobileos.Sensors")?
        .serve_at("/org/mobileos/Sensors", service)?
        .build()
        .await?;

    info!("sensors service running on session bus");

    std::future::pending::<()>().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use zbus::{connection, proxy, Connection};

    #[proxy(
        interface = "org.mobileos.Sensors",
        default_path = "/org/mobileos/Sensors"
    )]
    trait Sensors {
        #[zbus(property)]
        fn proximity(&self) -> zbus::Result<bool>;

        #[zbus(property)]
        fn ambient_light(&self) -> zbus::Result<u32>;

        #[zbus(property)]
        fn accelerometer_x(&self) -> zbus::Result<f64>;

        #[zbus(property)]
        fn accelerometer_y(&self) -> zbus::Result<f64>;

        #[zbus(property)]
        fn accelerometer_z(&self) -> zbus::Result<f64>;
    }

    async fn start_test_service() -> (Connection, zbus::names::OwnedUniqueName) {
        let service = super::SensorsService::new();
        let conn = connection::Builder::session()
            .unwrap()
            .serve_at("/org/mobileos/Sensors", service)
            .unwrap()
            .build()
            .await
            .unwrap();
        let name = conn.unique_name().unwrap().to_owned();
        (conn, name)
    }

    #[tokio::test]
    async fn reads_default_proximity() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = SensorsProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert!(!proxy.proximity().await.unwrap());
    }

    #[tokio::test]
    async fn reads_default_ambient_light() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = SensorsProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert_eq!(proxy.ambient_light().await.unwrap(), 500);
    }

    #[tokio::test]
    async fn reads_accelerometer_defaults() {
        let (_conn, name) = start_test_service().await;
        let client = Connection::session().await.unwrap();
        let proxy = SensorsProxy::builder(&client)
            .destination(name)
            .unwrap()
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();

        assert!((proxy.accelerometer_x().await.unwrap() - 0.0).abs() < f64::EPSILON);
        assert!((proxy.accelerometer_y().await.unwrap() - 0.0).abs() < f64::EPSILON);
        assert!((proxy.accelerometer_z().await.unwrap() - 9.8).abs() < f64::EPSILON);
    }
}
