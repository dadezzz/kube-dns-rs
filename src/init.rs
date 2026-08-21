use std::net::SocketAddr;

use kube::Client;
use tokio::net::{TcpListener, UdpSocket};
use tracing::info;

use crate::settings::Settings;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to load settings: {_0}")]
    LoadSettings(config::ConfigError),
    #[error("failed to bind UDP listener: {_0}")]
    BindUDP(std::io::Error),
    #[error("failed to bind TCP listener: {_0}")]
    BindTCP(std::io::Error),
    #[error("failed to start kubernetes client: {_0}")]
    LoadKubernetes(kube::Error),
}

pub fn start_logger() {
    tracing_subscriber::fmt().init();
}

pub fn load_settings(file: &str) -> Result<Settings, Error> {
    let settings = Settings::from_file(file).map_err(|e| Error::LoadSettings(e))?;
    info!("loaded configuration settings from {file}");
    Ok(settings)
}

pub async fn bind_listeners(
    udp_addr: SocketAddr,
    tcp_addr: SocketAddr,
) -> Result<(UdpSocket, TcpListener), Error> {
    let udp = UdpSocket::bind(udp_addr)
        .await
        .map_err(|e| Error::BindUDP(e))?;
    info!("started UDP listener on {udp_addr}");

    let tcp = TcpListener::bind(tcp_addr)
        .await
        .map_err(|e| Error::BindTCP(e))?;
    info!("started TCP listener on {tcp_addr}");

    Ok((udp, tcp))
}

pub async fn load_kubernetes() -> Result<Client, Error> {
    let client = Client::try_default()
        .await
        .map_err(|e| Error::LoadKubernetes(e))?;
    info!("loaded kubernetes client");
    Ok(client)
}
