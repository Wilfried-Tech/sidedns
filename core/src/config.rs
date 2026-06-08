use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::LazyLock,
};

pub const APP_NAME: &str = "SideDNS";
pub const APP_DIR_NAME: &str = APP_NAME;
pub const DAEMON_ENV: &str = "SIDEDNS_DAEMON_PROCESS";
pub const CERT_NAME: &str = "SideDNS Local CA";

pub const DNS_RECORD_TTL_SECONDS: u32 = 10;

pub const DNS_LISTEN_ADDR: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 53, 53)), 53);

pub const PROXY_LISTEN_ADDR: SocketAddr =
    SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 42)), 80);

#[cfg(target_os = "windows")]
pub const IPC_SOCKET_PATH: &str = r"\\.\pipe\sidedns";

#[cfg(not(target_os = "windows"))]
pub const IPC_SOCKET_PATH: &str = "/tmp/sidedns.sock";

pub static DOMAIN_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r"(?i)^(?:localhost|(?:\*\.)?(?:[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?\.)+[a-z]{2,63})$",
    )
    .unwrap()
});

pub static APP_DATA_DIR: LazyLock<std::path::PathBuf> = LazyLock::new(|| {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(APP_DIR_NAME.to_lowercase())
});

pub static ROOT_CERTIFICATE_DIR: LazyLock<std::path::PathBuf> =
    LazyLock::new(|| APP_DATA_DIR.join("Certificates"));
