use std::net::SocketAddr;

use config::{Config, ConfigError, File};
use hickory_server::proto::rr::LowerName;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct BlockerSettings {
    pub allowlist_urls: Vec<String>,
    pub blocklist_urls: Vec<String>,
}

#[derive(Deserialize)]
pub struct ListenerSettings {
    pub udp: SocketAddr,
    pub tcp: SocketAddr,
}

#[derive(Deserialize)]
pub struct KubernetesSettings {
    pub cluster_domain: LowerName,
}

#[derive(Deserialize)]
pub struct Settings {
    pub blocker: BlockerSettings,
    pub kubernetes: KubernetesSettings,
    pub listeners: ListenerSettings,
}

impl Settings {
    pub fn from_file(path: &str) -> Result<Self, ConfigError> {
        let config = Config::builder()
            .add_source(File::with_name(path))
            .build()?;

        config.try_deserialize()
    }
}
