use std::collections::HashMap;

use hickory_server::proto::rr::Name;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{trie::Trie, utils};

#[derive(Clone, Copy, PartialEq)]
pub enum BlockerDomainStatus {
    Blocked,
    Allowed,
}

#[derive(Default)]
pub struct BlockerContext {
    // Key is url of list.
    tries: HashMap<String, Trie<BlockerDomainStatus>>,
}

impl BlockerContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tries: HashMap::new(),
        }
    }

    // Replaces the old list if re-inserted.
    pub fn insert_list<'a, I>(&mut self, url: &str, list: I, status: BlockerDomainStatus)
    where
        I: IntoIterator<Item = &'a Name>,
    {
        let mut trie = Trie::new();

        list.into_iter()
            .map(utils::name_to_labels)
            .for_each(|labels| trie.insert(labels, status));

        self.tries.entry(url.to_owned()).insert_entry(trie);
    }

    #[must_use]
    pub fn status_of(&self, name: &Name) -> Option<(Vec<String>, Name, BlockerDomainStatus)> {
        let path = utils::name_to_labels(name);

        type Identity = (Vec<String>, Name, Option<BlockerDomainStatus>);
        let identity: Identity = (Vec::new(), Name::root(), None);

        let (urls, path, status): Identity = self
            .tries
            .par_iter()
            .map(|(url, trie)| {
                let (path, status) = trie.find_closest_prefix(path.clone());
                (
                    vec![url.to_owned()],
                    Name::from_labels(path).unwrap(),
                    status.copied(),
                )
            })
            .reduce(
                || identity.clone(),
                |mut acc_a, acc_b| {
                    if acc_a.1.len() > acc_b.1.len() {
                        // If there's a better match then it's the one that decides.
                        return acc_a;
                    } else if acc_a.1.len() < acc_b.1.len() {
                        // Worse matches are discarded.
                        return acc_b;
                    }

                    // Give max priority to allow lists, then blocklists.
                    match (acc_a.2, acc_b.2) {
                        (None, None) => acc_a, // Just re-use one of the 2, don't care which.
                        (None, Some(_))
                        | (
                            Some(BlockerDomainStatus::Blocked),
                            Some(BlockerDomainStatus::Allowed),
                        ) => acc_b,
                        (Some(_), None)
                        | (
                            Some(BlockerDomainStatus::Allowed),
                            Some(BlockerDomainStatus::Blocked),
                        ) => acc_a,
                        (
                            Some(BlockerDomainStatus::Allowed),
                            Some(BlockerDomainStatus::Allowed),
                        )
                        | (
                            Some(BlockerDomainStatus::Blocked),
                            Some(BlockerDomainStatus::Blocked),
                        ) => {
                            acc_a.0.extend(acc_b.0);
                            acc_a
                        }
                    }
                },
            );

        if let Some(status) = status {
            Some((urls, path, status))
        } else {
            None
        }
    }
}
