use serde::Deserialize;
use std::{
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    path::Path,
    str::FromStr,
    sync::Arc,
};

const LIBRARY_PATH: &str = "test ebooks";
const PROXY_KOBO_STORE: bool = true;
const DB_PATH: &str = "sync_db.redb";
const IMG_PATH: &str = "static/images";
const TEST_AUTH_KEY: &str = "test-key-123";
const BASE_URL: &str = "http://localhost:3000";
const HOST: &str = "0.0.0.0";
const PORT: u16 = 3000;
const MONGODB_NAME: &str = "ebbooks";

#[derive(Clone, Debug)]
pub struct AppConfig {
    pub library_path: Arc<Path>,
    pub proxy_kobo_store: bool,
    pub database_path: String,
    pub image_path: String,
    pub ebbooks_auth_key: String,
    pub base_url: String,
    pub mongodb_name: String,
    /// Fully resolved address (IP + port) to bind the server to.
    pub bind_addr: SocketAddr,
    /// Optional network interface name (e.g. "eth0", "wlan0") to bind the
    /// listening socket to. Linux-only (`SO_BINDTODEVICE`); applied when the
    /// listener is actually constructed, see `build_listener` below.
    // TODO Implement the network interface feature
    pub network_interface: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfigFile {
    library_path: Option<String>,
    proxy_kobo_store: Option<bool>,
    database_path: Option<String>,
    image_path: Option<String>,
    ebbooks_auth_key: Option<String>,
    base_url: Option<String>,
    mongodb_name: Option<String>,
    /// IP address or hostname, e.g. "0.0.0.0", "127.0.0.1", "::1", "localhost".
    host: Option<String>,
    port: Option<u16>,
    network_interface: Option<String>,
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        let config: ConfigFile = config::Config::builder()
            .add_source(config::File::with_name("config").required(false))
            .add_source(config::Environment::with_prefix("EBBOOKS"))
            .build()?
            .try_deserialize()?;

        let default = Self::default();
        let host = config.host.unwrap_or_else(|| HOST.to_string());
        let port = config.port.unwrap_or(PORT);
        let bind_addr = resolve_bind_addr(&host, port)
            .map_err(|e| config::ConfigError::Message(e.to_string()))?;

        Ok(Self {
            library_path: config
                .library_path
                .map(|path| Arc::from(Path::new(&path)))
                .unwrap_or(default.library_path),
            proxy_kobo_store: config.proxy_kobo_store.unwrap_or(default.proxy_kobo_store),
            database_path: config.database_path.unwrap_or(default.database_path),
            mongodb_name: config.mongodb_name.unwrap_or(default.mongodb_name),
            image_path: config.image_path.unwrap_or(default.image_path),
            ebbooks_auth_key: config.ebbooks_auth_key.unwrap_or(default.ebbooks_auth_key),
            base_url: config.base_url.unwrap_or(default.base_url),
            bind_addr,
            network_interface: config.network_interface,
        })
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            library_path: Arc::from(Path::new(LIBRARY_PATH)),
            proxy_kobo_store: PROXY_KOBO_STORE,
            database_path: DB_PATH.to_string(),
            mongodb_name: MONGODB_NAME.to_string(),
            image_path: IMG_PATH.to_string(),
            ebbooks_auth_key: TEST_AUTH_KEY.to_string(),
            base_url: BASE_URL.to_string(),
            bind_addr: resolve_bind_addr(HOST, PORT).expect("default HOST/PORT must be valid"),
            network_interface: None,
        }
    }
}

impl AppConfig {
    pub fn new(
        library_path: Option<impl AsRef<Path>>,
        proxy_kobo_store: Option<bool>,
        database_path: Option<impl Into<String>>,
        mongodb_name: Option<impl Into<String>>,
        image_path: Option<impl Into<String>>,
        ebbooks_auth_key: Option<impl Into<String>>,
        base_url: Option<impl Into<String>>,
        bind_addr: Option<SocketAddr>,
        network_interface: Option<String>,
    ) -> Self {
        let default = Self::default();

        Self {
            library_path: library_path
                .map(|p| Arc::from(p.as_ref()))
                .unwrap_or(default.library_path),
            proxy_kobo_store: proxy_kobo_store.unwrap_or(default.proxy_kobo_store),
            database_path: database_path
                .map(Into::into)
                .unwrap_or(default.database_path),
            mongodb_name: mongodb_name.map(Into::into).unwrap_or(default.mongodb_name),
            image_path: image_path.map(Into::into).unwrap_or(default.image_path),
            ebbooks_auth_key: ebbooks_auth_key
                .map(Into::into)
                .unwrap_or(default.ebbooks_auth_key),
            base_url: base_url.map(Into::into).unwrap_or(default.base_url),
            bind_addr: bind_addr.unwrap_or(default.bind_addr),
            network_interface: network_interface.or(default.network_interface),
        }
    }
}

/// Address or hostname to listen on, e.g. "0.0.0.0", "127.0.0.1",
/// "::1", "[::1]", or "localhost".
fn resolve_bind_addr(host: &str, port: u16) -> std::io::Result<SocketAddr> {
    let host = host.trim();

    if host.is_empty() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "bind host cannot be empty",
        ));
    }

    // Accept bracketed IPv6 addresses, e.g. "[::1]".
    let host = host
        .strip_prefix('[')
        .and_then(|host| host.strip_suffix(']'))
        .unwrap_or(host);

    // Avoid DNS resolution when the host is already an IP address.
    if let Ok(ip) = IpAddr::from_str(host) {
        return Ok(SocketAddr::new(ip, port));
    }

    (host, port).to_socket_addrs()?.next().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::AddrNotAvailable,
            format!("could not resolve bind host '{host}'"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_ipv4() {
        assert_eq!(
            resolve_bind_addr("127.0.0.1", 3000).unwrap(),
            "127.0.0.1:3000".parse().unwrap()
        );
    }

    #[test]
    fn resolves_ipv6() {
        assert_eq!(
            resolve_bind_addr("::1", 3000).unwrap(),
            "[::1]:3000".parse().unwrap()
        );
    }

    #[test]
    fn resolves_bracketed_ipv6() {
        assert_eq!(
            resolve_bind_addr("[::1]", 3000).unwrap(),
            "[::1]:3000".parse().unwrap()
        );
    }

    #[test]
    fn rejects_empty_host() {
        assert!(resolve_bind_addr("", 3000).is_err());
        assert!(resolve_bind_addr("   ", 3000).is_err());
    }

    #[test]
    fn resolves_hostname() {
        let addr = resolve_bind_addr("localhost", 3000).unwrap();
        assert_eq!(addr.port(), 3000);
    }
}
