use std::collections::HashMap;

use hickory_server::proto::rr::Name;
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{trie::Trie, utils};

#[derive(Clone, Copy, PartialEq)]
pub enum ListType {
    Block,
    Allow,
}

#[derive(Default)]
pub struct BlockerContext {
    // Key is url of list.
    tries: HashMap<String, Trie<ListType>>,
}

impl BlockerContext {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tries: HashMap::new(),
        }
    }

    // Replaces the old list if re-inserted.
    pub fn insert_list<'a, I>(&mut self, url: &str, list: I, list_type: ListType)
    where
        I: IntoIterator<Item = &'a Name>,
    {
        let mut trie = Trie::new();

        list.into_iter()
            .map(utils::name_to_labels)
            .for_each(|labels| trie.insert(labels, list_type));

        self.tries.entry(url.to_owned()).insert_entry(trie);
    }

    #[must_use]
    pub fn lookup(&self, name: &Name) -> Option<(Vec<String>, Name, ListType)> {
        let path = utils::name_to_labels(name);

        type Identity = (Vec<String>, Name, Option<ListType>);
        let identity: Identity = (Vec::new(), Name::root(), None);

        let (urls, path, list_type): Identity = self
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
                        (None, Some(_)) | (Some(ListType::Block), Some(ListType::Allow)) => acc_b,
                        (Some(_), None) | (Some(ListType::Allow), Some(ListType::Block)) => acc_a,
                        (Some(ListType::Allow), Some(ListType::Allow))
                        | (Some(ListType::Block), Some(ListType::Block)) => {
                            acc_a.0.extend(acc_b.0);
                            acc_a
                        }
                    }
                },
            );

        list_type.map(|list_type| (urls, path, list_type))
    }
}
