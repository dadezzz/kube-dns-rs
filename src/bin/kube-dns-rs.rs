use std::sync::Arc;

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

use kube_dns_rs::{
    blocker::{context::BlockerContext, handler::BlockerZoneHandler},
    init,
    kubernetes::crd::{
        context::KubernetesCrdContext, handler::KubernetesCrdZoneHandler,
        watcher::KubernetesCrdWatcher,
    },
};

async fn run() -> Result<(), init::Error> {
    init::start_logger();

    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .unwrap();
    let tls_root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let tls_client = rustls::ClientConfig::builder()
        .with_root_certificates(tls_root_store)
        .with_no_client_auth();

    let args = Args::parse();

    let settings = init::load_settings(&args.config)?;
    let k8s_client = init::load_kubernetes().await?;

    let resolver_handler = Arc::new(ResolverZoneHandler::new(
        &settings.upstreams,
        tls_client.clone(),
    ));

    let blocker_context: Arc<RwLock<BlockerContext>> = Arc::default();
    let mut blocker_refresher = BlockerRefresher::new(
        blocker_context.clone(),
        tls_client,
        resolver_handler.resolver.clone(),
    );

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

    let blocker_handler = Arc::new(BlockerZoneHandler::new(blocker_context));

    let mut fq_cluster_domain = settings.kubernetes.cluster_domain;
    fq_cluster_domain.set_fqdn(true);

    let k8s_svc_ctx: Arc<RwLock<KubernetesSvcContext>> = Arc::default();
    let mut k8s_svc_watcher = KubernetesSvcWatcher::new(k8s_client.clone(), k8s_svc_ctx.clone());
    let k8s_svc_handler = Arc::new(KubernetesSvcZoneHandler::new(
        fq_cluster_domain.clone(),
        k8s_svc_ctx,
    ));

    let mut k8s_crd_catalog = Catalog::new();
    k8s_crd_catalog.upsert(
        Name::root().into(),
        vec![blocker_handler.clone(), resolver_handler.clone()],
    );
    k8s_crd_catalog.upsert(fq_cluster_domain.clone(), vec![k8s_svc_handler.clone()]);

    let k8s_crd_ctx: Arc<RwLock<KubernetesCrdContext>> = Arc::default();
    let mut k8s_crd_watcher = KubernetesCrdWatcher::new(k8s_client, k8s_crd_ctx.clone());
    let k8s_crd_handler = Arc::new(KubernetesCrdZoneHandler::new(k8s_crd_catalog, k8s_crd_ctx));

    let mut catalog = Catalog::new();
    catalog.upsert(
        Name::root().into(),
        vec![k8s_crd_handler, blocker_handler, resolver_handler],
    );
    catalog.upsert(fq_cluster_domain, vec![k8s_svc_handler]);

    let mut server = hickory_server::Server::new(catalog);
    init::bind_listeners(&mut server, &settings.listeners).await?;

    tokio::select! {
        _ = blocker_refresher.run() =>(),
        _ = k8s_crd_watcher.run() => (),
        _ = k8s_svc_watcher.run() => (),
        _ = tokio::signal::ctrl_c() => ()
    };

    tracing::warn!("shutting down");
    server.shutdown_gracefully().await.unwrap();

    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        tracing::error!("{e}");
    }
}
