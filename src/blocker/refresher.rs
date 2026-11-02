use std::{net::SocketAddr, str::FromStr, sync::Arc, time::Duration};

use hickory_server::proto::{ProtoError, rr::Name};
use rayon::{iter::ParallelIterator, str::ParallelString};
use tokio::{
    sync::RwLock,
    task::{JoinError, JoinSet},
    time::interval,
};

use crate::blocker::context::ListType;

use super::context::BlockerContext;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to fetch list: {0}")]
    ListFetch(reqwest::Error),
    #[error("failed to parse DNS name: {0}")]
    NameParse(ProtoError),
    #[error("failed to join task: {0}")]
    TaskJoin(JoinError),
}

#[derive(Clone)]
struct ReqwestResolver {
    resolver: Arc<hickory_resolver::TokioResolver>,
}

impl reqwest::dns::Resolve for ReqwestResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let self_cloned = self.clone();

        Box::pin(async move {
            let lookup = self_cloned
                .resolver
                .lookup_ip(Name::from_utf8(name.as_str()).unwrap())
                .await?;

            let ips = lookup.into_iter().map(|ip| SocketAddr::new(ip, 0));
            return Ok(Box::new(ips.into_iter()) as reqwest::dns::Addrs);
        })
    }
}

struct BlockerRefresherTaskContext {
    http_client: reqwest::Client,
    context: Arc<RwLock<BlockerContext>>,
    status: ListType,
    url: String,
}

impl BlockerRefresherTaskContext {
    async fn fetch_list(&self, url: &str) -> Result<Vec<Name>, Error> {
        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(Error::ListFetch)?;

        let body = response.text().await.map_err(Error::ListFetch)?;

        let names: Vec<_> = body
            .par_lines()
            .filter_map(|line| {
                let line = line.trim();

                // Skip comments and empty lines.
                if line.starts_with('#') || line.is_empty() {
                    return None;
                }

                // Remove wildcards.
                let line = line.trim_start_matches("*.");

                let name = Name::from_str(line).map_err(Error::NameParse);
                Some(name)
            })
            .collect::<Result<_, _>>()?;

        Ok(names)
    }

    async fn fetch_and_insert(&self) -> Result<(), Error> {
        let lines = self.fetch_list(&self.url).await?;

        self.context
            .write()
            .await
            .insert_list(&self.url, lines.iter(), self.status);

        tracing::info!("refreshed list {}, found {} domains", self.url, lines.len());
        Ok(())
    }
}

pub struct BlockerRefresher {
    // Maps url to next update time.
    tasks: JoinSet<Result<(), Error>>,
    http_client: reqwest::Client,
    context: Arc<RwLock<BlockerContext>>,
}

impl BlockerRefresher {
    pub fn new(
        context: Arc<RwLock<BlockerContext>>,
        tls_client: rustls::ClientConfig,
        resolver: Arc<hickory_resolver::TokioResolver>,
    ) -> Self {
        let resolver = ReqwestResolver { resolver };

        Self {
            tasks: JoinSet::new(),
            http_client: reqwest::Client::builder()
                .dns_resolver(resolver)
                .tls_backend_preconfigured(tls_client)
                .gzip(true)
                .build()
                .unwrap(),
            context,
        }
    }

    async fn add_list(&mut self, url: &str, status: ListType) -> Result<(), Error> {
        let task_context = BlockerRefresherTaskContext {
            status,
            url: url.to_owned(),
            context: self.context.clone(),
            http_client: self.http_client.clone(),
        };

        // Refresh lists every 48 hours. TODO: make configurable.
        let mut interval = interval(Duration::from_hours(48));
        // Download the list synchronously the first time, so that it's already
        // blocking when listeners start.
        interval.tick().await;
        task_context.fetch_and_insert().await?;

        self.tasks.spawn(async move {
            const DEFAULT_RETRIES: u64 = 5;
            let mut retries_left = DEFAULT_RETRIES;

            loop {
                interval.tick().await;

                if let Err(e) = task_context.fetch_and_insert().await {
                    if retries_left == 0 {
                        return Err(e);
                    }

                    retries_left -= 1;
                    tracing::error!("{e}");
                    // Retry after a minute.
                    interval.reset_after(Duration::from_secs(60));
                } else {
                    retries_left = DEFAULT_RETRIES;
                }
            }
        });

        Ok(())
    }

    pub async fn add_block_list(&mut self, url: &str) -> Result<(), Error> {
        self.add_list(url, ListType::Block).await?;
        tracing::info!("added new block list {url}");
        Ok(())
    }

    pub async fn add_allow_list(&mut self, url: &str) -> Result<(), Error> {
        self.add_list(url, ListType::Allow).await?;
        tracing::info!("added new allow list {url}");
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        if self.tasks.is_empty() {
            // Handle case where there are no lists. The server needs to keep
            // running.
            self.tasks.spawn(std::future::pending());
        }

        while let Some(res) = self.tasks.join_next().await {
            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(e) => return Err(Error::TaskJoin(e)),
            }
        }

        Ok(())
    }
}

impl Drop for BlockerRefresher {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}
