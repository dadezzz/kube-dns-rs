use std::{fmt::Display, net::SocketAddr, path::PathBuf};

use config::{Config, ConfigError, File};
use hickory_server::proto::rr::LowerName;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct BlockerSettings {
    pub allowlist_urls: Vec<String>,
    pub blocklist_urls: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum UpstreamSettings {
    UDP {
        addr: SocketAddr,
        trust_negative_responses: bool,
    },
    TCP {
        addr: SocketAddr,
        trust_negative_responses: bool,
    },
    TLS {
        addr: SocketAddr,
        trust_negative_responses: bool,
        hostname: String,
    },
    HTTPS {
        addr: SocketAddr,
        trust_negative_responses: bool,
        hostname: String,
        path: String,
    },
    QUIC {
        addr: SocketAddr,
        trust_negative_responses: bool,
        hostname: String,
    },
    HTTP3 {
        addr: SocketAddr,
        trust_negative_responses: bool,
        hostname: String,
        path: String,
    },
}

#[derive(Deserialize)]
#[serde(tag = "type")]
pub enum ListenerSettings {
    UDP {
        addr: SocketAddr,
    },
    TCP {
        addr: SocketAddr,
        timeout: u64,
    },
    TLS {
        addr: SocketAddr,
        timeout: u64,
        tls_cert_file: PathBuf,
        tls_key_file: PathBuf,
    },
    HTTPS {
        addr: SocketAddr,
        timeout: u64,
        tls_cert_file: PathBuf,
        tls_key_file: PathBuf,
        hostname: Option<String>,
        path: String,
    },
    QUIC {
        addr: SocketAddr,
        timeout: u64,
        tls_cert_file: PathBuf,
        tls_key_file: PathBuf,
    },
    HTTP3 {
        addr: SocketAddr,
        timeout: u64,
        tls_cert_file: PathBuf,
        tls_key_file: PathBuf,
        hostname: Option<String>,
    },
}

impl Display for ListenerSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ListenerSettings::UDP { addr } => f.write_fmt(format_args!("UDP on {addr}")),
            ListenerSettings::TCP { addr, .. } => f.write_fmt(format_args!("TCP on {addr}")),
            ListenerSettings::TLS { addr, .. } => f.write_fmt(format_args!("TLS on {addr}")),
            ListenerSettings::HTTPS { addr, .. } => f.write_fmt(format_args!("HTTPS on {addr}")),
            ListenerSettings::QUIC { addr, .. } => f.write_fmt(format_args!("QUIC on {addr}")),
            ListenerSettings::HTTP3 { addr, .. } => f.write_fmt(format_args!("HTTP3 on {addr}")),
        }
    }
}

#[derive(Deserialize)]
pub struct KubernetesSettings {
    pub cluster_domain: LowerName,
}

#[derive(Deserialize)]
pub struct Settings {
    pub blocker: BlockerSettings,
    pub kubernetes: KubernetesSettings,
    pub listeners: Vec<ListenerSettings>,
    pub upstreams: Vec<UpstreamSettings>,
}

impl Settings {
    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(path))
            .build()?;

        config.try_deserialize()
    }
}

#[cfg(test)]
mod test {
    use config::{Config, File};

    use super::Settings;

    #[test]
    fn try_load_example_config() {
        let config = Config::builder()
            .add_source(File::with_name("examples/config.yaml"))
            .build()
            .unwrap();
        config.try_deserialize::<Settings>().unwrap();
    }
}
