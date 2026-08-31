use std::{sync::Arc, time::Duration};

use clap::Parser;
use hickory_server::{proto::rr::Name, zone_handler::Catalog};
use kube_dns_rs::{
    args::Args,
    blocker::refresher::BlockerRefresher,
    kubernetes::svc::{
        context::KubernetesSvcContext, handler::KubernetesSvcZoneHandler,
        watcher::KubernetesSvcWatcher,
    },
    resolver::ResolverZoneHandler,
};
use tokio::sync::RwLock;
use tracing::{info, warn};

use kube_dns_rs::{
    blocker::{context::BlockerContext, handler::BlockerZoneHandler},
    init,
    kubernetes::crd::{
        context::KubernetesCrdContext, handler::KubernetesCrdZoneHandler,
        watcher::KubernetesCrdWatcher,
    },
};

#[tokio::main]
async fn main() -> Result<(), init::Error> {
    init::start_logger();
    let args = Args::parse();

    let settings = init::load_settings(&args.config)?;
    info!("loaded config from {}", args.config);

    let k8s_client = init::load_kubernetes().await?;

    let mut catalog = Catalog::new();

    let blocker_context: Arc<RwLock<BlockerContext>> = Arc::default();
    let mut blocker_refresher = BlockerRefresher::new(blocker_context.clone());

    for block_list_url in settings.blocker.blocklist_urls {
        blocker_refresher
            .add_block_list(block_list_url.as_str())
            .await
            .map_err(init::Error::DownloadList)?;
    }

    for allow_list_url in settings.blocker.allowlist_urls {
        blocker_refresher
            .add_allow_list(allow_list_url.as_str())
            .await
            .map_err(init::Error::DownloadList)?;
    }

    let blocker_handler = BlockerZoneHandler::new(blocker_context);
    let resolver_handler = ResolverZoneHandler::new();

    let k8s_crd_ctx: Arc<RwLock<KubernetesCrdContext>> = Arc::default();
    let mut k8s_crd_watcher = KubernetesCrdWatcher::new(k8s_client.clone(), k8s_crd_ctx.clone());
    let k8s_crd_handler = KubernetesCrdZoneHandler::new(k8s_crd_ctx);

    catalog.upsert(
        Name::root().into(),
        vec![
            Arc::new(k8s_crd_handler),
            Arc::new(blocker_handler),
            Arc::new(resolver_handler),
        ],
    );

    let mut fq_cluster_domain = settings.kubernetes.cluster_domain;
    fq_cluster_domain.set_fqdn(true);

    let k8s_svc_ctx: Arc<RwLock<KubernetesSvcContext>> = Arc::default();
    let mut k8s_svc_watcher = KubernetesSvcWatcher::new(k8s_client, k8s_svc_ctx.clone());
    let k8s_svc_handler = KubernetesSvcZoneHandler::new(fq_cluster_domain.clone(), k8s_svc_ctx);

    catalog.upsert(fq_cluster_domain, vec![Arc::new(k8s_svc_handler)]);

    let mut server = hickory_server::Server::new(catalog);
    let (udp, tcp) = init::bind_listeners(settings.listeners.tcp, settings.listeners.udp).await?;
    server.register_socket(udp);
    // Values taken from hickory's server implementation.
    server.register_listener(tcp, Duration::from_secs(10), 32);

    tokio::select! {
        _ = blocker_refresher.run() =>(),
        _ = k8s_crd_watcher.run() => (),
        _ = k8s_svc_watcher.run() => (),
        _ = tokio::signal::ctrl_c() => ()
    };

    warn!("shutting down");
    server.shutdown_gracefully().await.unwrap();

    Ok(())
}
