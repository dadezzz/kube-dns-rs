use std::{path::Path, sync::Arc, time::Duration};

use hickory_resolver::config::{ConnectionConfig, NameServerConfig};
use kube::Client;
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
};
use tokio::net::{TcpListener, UdpSocket};

use crate::{
    blocker,
    settings::{ListenerSettings, Settings, UpstreamSettings},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to load settings: {0}")]
    LoadSettings(config::ConfigError),
    #[error("failed to bind listener: {0}")]
    BindListener(std::io::Error),
    #[error("failed to set up TLS server: {0}")]
    RustlsConfig(rustls::Error),
    #[error("failed to bind listener: {0}")]
    BindNetListener(hickory_server::net::NetError),
    #[error("failed to load certificate/key file: {0}")]
    LoadPemFile(rustls::pki_types::pem::Error),
    #[error("failed to start kubernetes client: {0}")]
    LoadKubernetes(kube::Error),
    #[error("failed to fetch domain list: {0}")]
    DownloadList(blocker::refresher::Error),
}

pub fn start_logger() {
    tracing_subscriber::fmt().init();
}

pub fn load_settings(file: &str) -> Result<Settings, Error> {
    let settings = Settings::from_file(file).map_err(Error::LoadSettings)?;
    tracing::info!("loaded configuration settings from {file}");
    Ok(settings)
}

fn load_tls_server_config(
    tls_cert_file: &Path,
    tls_key_file: &Path,
) -> Result<ServerConfig, Error> {
    let tls_cert_chain = CertificateDer::pem_file_iter(tls_cert_file)
        .map_err(Error::LoadPemFile)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(Error::LoadPemFile)?;

    let tls_key = PrivateKeyDer::from_pem_file(tls_key_file).map_err(Error::LoadPemFile)?;

    ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(tls_cert_chain, tls_key)
        .map_err(Error::RustlsConfig)
}

pub async fn bind_listeners<T: hickory_server::server::RequestHandler>(
    server: &mut hickory_server::Server<T>,
    listeners: &[ListenerSettings],
) -> Result<(), Error> {
    for l in listeners {
        match l {
            ListenerSettings::UDP { addr } => {
                let socket = UdpSocket::bind(addr.to_owned())
                    .await
                    .map_err(Error::BindListener)?;

                server.register_socket(socket);
            }
            ListenerSettings::TCP { addr, timeout } => {
                let listener = TcpListener::bind(addr.to_owned())
                    .await
                    .map_err(Error::BindListener)?;

                server.register_listener(listener, Duration::from_secs(*timeout), 32);
            }
            ListenerSettings::TLS {
                addr,
                timeout,
                tls_cert_file,
                tls_key_file,
            } => {
                let listener = TcpListener::bind(addr.to_owned())
                    .await
                    .map_err(Error::BindListener)?;

                let tls_config = load_tls_server_config(tls_cert_file, tls_key_file)?;

                server
                    .register_tls_listener_with_tls_config(
                        listener,
                        Duration::from_secs(*timeout),
                        Arc::new(tls_config),
                    )
                    .map_err(Error::BindListener)?;
            }
            ListenerSettings::HTTPS {
                addr,
                timeout,
                tls_cert_file,
                tls_key_file,
                hostname,
                path,
            } => {
                let listener = TcpListener::bind(addr.to_owned())
                    .await
                    .map_err(Error::BindListener)?;

                let tls_config = load_tls_server_config(tls_cert_file, tls_key_file)?;

                server
                    .register_https_listener_with_tls_config(
                        listener,
                        Duration::from_secs(*timeout),
                        Arc::new(tls_config),
                        hostname.to_owned(),
                        path.to_owned(),
                    )
                    .map_err(Error::BindListener)?;
            }
            ListenerSettings::QUIC {
                addr,
                timeout,
                tls_cert_file,
                tls_key_file,
            } => {
                let socket = UdpSocket::bind(addr.to_owned())
                    .await
                    .map_err(Error::BindListener)?;

                let tls_config = load_tls_server_config(tls_cert_file, tls_key_file)?;

                server
                    .register_quic_listener_and_tls_config(
                        socket,
                        Duration::from_secs(*timeout),
                        Arc::new(tls_config),
                    )
                    .map_err(Error::BindNetListener)?;
            }
            ListenerSettings::HTTP3 {
                addr,
                timeout,
                tls_cert_file,
                tls_key_file,
                hostname,
            } => {
                let socket = UdpSocket::bind(addr.to_owned())
                    .await
                    .map_err(Error::BindListener)?;

                let tls_config = load_tls_server_config(tls_cert_file, tls_key_file)?;

                server
                    .register_h3_listener_with_tls_config(
                        socket,
                        Duration::from_secs(*timeout),
                        Arc::new(tls_config),
                        hostname.to_owned(),
                    )
                    .map_err(Error::BindNetListener)?;
            }
        }

        tracing::info!("created listener {l}");
    }

    Ok(())
}

#[must_use]
pub fn configure_upstreams(upstreams: &[UpstreamSettings]) -> Vec<NameServerConfig> {
    upstreams
        .iter()
        .map(|u| match u {
            UpstreamSettings::UDP {
                addr,
                trust_negative_responses,
            } => {
                let mut connection = ConnectionConfig::udp();
                connection.port = addr.port();
                NameServerConfig::new(addr.ip(), *trust_negative_responses, vec![connection])
            }
            UpstreamSettings::TCP {
                addr,
                trust_negative_responses,
            } => {
                let mut connection = ConnectionConfig::tcp();
                connection.port = addr.port();
                NameServerConfig::new(addr.ip(), *trust_negative_responses, vec![connection])
            }
            UpstreamSettings::TLS {
                addr,
                trust_negative_responses,
                hostname,
            } => {
                let mut connection = ConnectionConfig::tls(Arc::from(hostname.as_str()));
                connection.port = addr.port();
                NameServerConfig::new(addr.ip(), *trust_negative_responses, vec![connection])
            }
            UpstreamSettings::HTTPS {
                addr,
                trust_negative_responses,
                hostname,
                path,
            } => {
                let mut connection = ConnectionConfig::https(
                    Arc::from(hostname.as_str()),
                    Some(Arc::from(path.as_str())),
                );
                connection.port = addr.port();
                NameServerConfig::new(addr.ip(), *trust_negative_responses, vec![connection])
            }
            UpstreamSettings::QUIC {
                addr,
                trust_negative_responses,
                hostname,
            } => {
                let mut connection = ConnectionConfig::quic(Arc::from(hostname.as_str()));
                connection.port = addr.port();
                NameServerConfig::new(addr.ip(), *trust_negative_responses, vec![connection])
            }
            UpstreamSettings::HTTP3 {
                addr,
                trust_negative_responses,
                hostname,
                path,
            } => {
                let mut connection = ConnectionConfig::h3(
                    Arc::from(hostname.as_str()),
                    Some(Arc::from(path.as_str())),
                );
                connection.port = addr.port();
                NameServerConfig::new(addr.ip(), *trust_negative_responses, vec![connection])
            }
        })
        .collect()
}

pub async fn load_kubernetes() -> Result<Client, Error> {
    let client = Client::try_default().await.map_err(Error::LoadKubernetes)?;
    tracing::info!("loaded kubernetes client");
    Ok(client)
}
