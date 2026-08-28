use std::{str::FromStr, sync::Arc};

use hickory_server::proto::rr::Name;
use rayon::{iter::ParallelIterator, str::ParallelString};
use tokio::{
    sync::RwLock,
    task::{JoinError, JoinSet},
};
use tracing::info;

use crate::blocker::context::BlockerDomainStatus;

use super::context::BlockerContext;

#[derive(thiserror::Error, Debug)]
pub enum BlockerRefresherError {
    #[error("failed to fetch list: {0}")]
    ListFetch(reqwest::Error),
    #[error("failed to join task: {0}")]
    TaskJoin(JoinError),
}

pub struct BlockerListRefresher {
    // Maps url to next update time.
    tasks: JoinSet<()>,
    http_client: reqwest::Client,
    context: Arc<RwLock<BlockerContext>>,
}

impl BlockerListRefresher {
    pub fn new(context: Arc<RwLock<BlockerContext>>) -> Self {
        Self {
            tasks: JoinSet::new(),
            http_client: reqwest::Client::new(),
            context,
        }
    }

    async fn fetch_list(&self, url: &str) -> Result<Vec<Name>, BlockerRefresherError> {
        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(BlockerRefresherError::ListFetch)?;
        let body = response
            .text()
            .await
            .map_err(BlockerRefresherError::ListFetch)?;

        let lines = body
            .par_lines()
            .filter_map(|l| {
                let l = l.trim();

                // Skip comments and empty lines.
                if l.starts_with('#') || l.is_empty() {
                    return None;
                }

                // Remove wildcards.
                let l = l.trim_start_matches("*.");

                Some(Name::from_str(l).unwrap())
            })
            .collect();

        Ok(lines)
    }

    async fn add_list<'a, I>(&mut self, url: &str, lines: I, status: BlockerDomainStatus)
    where
        I: IntoIterator<Item = &'a Name>,
    {
        self.tasks.spawn(async { std::future::pending().await });
        self.context.write().await.insert_list(url, lines, status);
    }

    pub async fn add_block_list(&mut self, url: &str) -> Result<(), BlockerRefresherError> {
        let lines = self.fetch_list(url).await?;
        self.add_list(url, lines.iter(), BlockerDomainStatus::Blocked)
            .await;
        info!("added new block list {url} ({} domains)", lines.len());
        Ok(())
    }

    pub async fn add_allow_list(&mut self, url: &str) -> Result<(), BlockerRefresherError> {
        let lines = self.fetch_list(url).await?;
        self.add_list(url, lines.iter(), BlockerDomainStatus::Blocked)
            .await;
        info!("added new allow list {url}");
        Ok(())
    }

    pub async fn run(&mut self) -> Result<(), BlockerRefresherError> {
        if self.tasks.is_empty() {
            // Handle case where there are no lists. The server needs to keep
            // running.
            self.tasks.spawn(std::future::pending());
        }

        while let Some(res) = self.tasks.join_next().await {
            match res {
                Ok(()) => continue,
                Err(e) => return Err(BlockerRefresherError::TaskJoin(e)),
            }
        }

        Ok(())
    }
}

impl Drop for BlockerListRefresher {
    fn drop(&mut self) {
        self.tasks.abort_all();
    }
}
