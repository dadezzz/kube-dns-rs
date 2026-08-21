use std::{collections::HashMap, str::FromStr};

use hickory_server::proto::rr::Name;
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    str::ParallelString,
};

use crate::{trie::Trie, utils};

#[derive(Clone, Copy, PartialEq)]
pub enum Status {
    Blocked,
    Allowed,
}

pub struct BlockListZoneHandlerDomains {
    client: reqwest::Client,
    // Key is url of list.
    tries: HashMap<String, Trie<Status>>,
}

impl BlockListZoneHandlerDomains {
    #[must_use]
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
            tries: HashMap::new(),
        }
    }

    fn add_list(&mut self, url: &str, list: &[Name], status: Status) {
        let mut trie = Trie::new();

        list.iter()
            .map(utils::name_to_labels)
            .for_each(|labels| trie.insert(labels, status));

        self.tries.insert(url.to_owned(), trie);
    }

    async fn fetch_list(&self, url: &str) -> Vec<Name> {
        println!("parsing {url}");
        let response = self.client.get(url).send().await.unwrap();
        let body = response.text().await.unwrap();

        body.par_lines()
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
            .collect()
    }

    pub async fn add_allow_list(&mut self, url: &str) {
        let list = self.fetch_list(url).await;
        self.add_list(url, &list, Status::Allowed);
    }

    pub async fn add_block_list(&mut self, url: &str) {
        let list = self.fetch_list(url).await;
        self.add_list(url, &list, Status::Blocked);
    }

    #[must_use]
    pub fn status_of(&self, name: &Name) -> (Vec<(String, Name)>, Status) {
        let path = utils::name_to_labels(name);

        // Store depth and value.
        let values: Vec<_> = self
            .tries
            .par_iter()
            .map(|(url, trie)| {
                let (path, status) = trie.find_closest_prefix(path.clone());
                (url, path, status)
            })
            .collect();

        let (deciders, status) =
            values
                .into_iter()
                .fold((Vec::new(), None), |mut acc, (url, path, status)| {
                    match status {
                        None => return acc,
                        Some(Status::Blocked) => {
                            if acc.1 == Some(Status::Allowed) {
                                return acc;
                            } else if acc.1.is_none() {
                                acc.0 = Vec::new();
                                acc.1 = Some(Status::Blocked);
                            }
                        }
                        Some(Status::Allowed) => {
                            if acc.1 != Some(Status::Allowed) {
                                acc.0 = Vec::new();
                                acc.1 = Some(Status::Allowed);
                            }
                        }
                    }

                    let name = Name::from_labels(path.into_iter().rev()).unwrap();
                    acc.0.push((url.to_owned(), name));
                    acc
                });

        (deciders, status.unwrap_or(Status::Allowed))
    }
}
