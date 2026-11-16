use std::{collections::HashMap, fmt::Debug};

#[derive(Debug)]
struct TrieNode<V> {
    value: Option<V>,
    children: HashMap<String, Self>,
}

#[derive(Debug)]
pub struct Trie<V> {
    root: TrieNode<V>,
}

impl<V> Trie<V> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            root: TrieNode {
                value: None,
                children: HashMap::new(),
            },
        }
    }

    pub fn insert<'a, I, S>(&mut self, path: &'a I, value: V)
    where
        &'a I: IntoIterator<Item = S>,
        S: AsRef<str> + ToString,
    {
        let mut current_node = &mut self.root;

        for key in path {
            current_node = current_node
                .children
                .entry(key.to_string())
                .or_insert_with(|| TrieNode {
                    value: None,
                    children: HashMap::new(),
                });
        }

        current_node.value = Some(value);
    }

    pub fn remove<'a, I, S>(&mut self, path: &'a I) -> Option<V>
    where
        &'a I: IntoIterator<Item = S>,
        S: AsRef<str> + Debug,
    {
        let mut path_iter = path.into_iter().peekable();
        let mut parent_node = &mut self.root;

        let mut last_key = None;
        while let Some(key) = path_iter.next() {
            if path_iter.peek().is_some() {
                parent_node = parent_node.children.get_mut(key.as_ref())?;
            } else {
                last_key = Some(key);
            }
        }

        last_key.and_then(|last_key| {
            let child_node = parent_node.children.get_mut(last_key.as_ref())?;
            let child_node_value = child_node.value.take();

            // If the node doesn't have childrens then we can remove it.
            if child_node.children.is_empty() {
                parent_node.children.remove(last_key.as_ref());
            }

            child_node_value
        })
    }

    pub fn get_mut<'a, I, S>(&mut self, path: &'a I) -> Option<&mut V>
    where
        &'a I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut node = &mut self.root;

        for key in path {
            node = node.children.get_mut(key.as_ref())?;
        }

        node.value.as_mut()
    }

    pub fn get<'a, I, S>(&self, path: &'a I) -> Option<&V>
    where
        &'a I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut node = &self.root;

        for key in path {
            node = node.children.get(key.as_ref())?;
        }

        node.value.as_ref()
    }

    pub fn find_closest_prefix<'a, I, S>(&self, path: &'a I) -> (Vec<&str>, Option<&V>)
    where
        &'a I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut matched_path = Vec::new();
        let mut matched_node = &self.root;

        for key in path {
            if let Some((key, child)) = matched_node.children.get_key_value(key.as_ref()) {
                matched_path.push(key.as_str());
                matched_node = child;
            } else {
                break;
            }
        }

        (matched_path, matched_node.value.as_ref())
    }
}

impl<V> Default for Trie<V> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {
    use super::Trie;

    fn create_trie() -> Trie<bool> {
        let mut trie = Trie::<bool>::new();
        trie.insert(&["a", "b"], true);
        return trie;
    }

    #[test]
    fn doesnt_match_parent_path() {
        let trie = create_trie();
        let (matched_path, value) = trie.find_closest_prefix(&["a"]);
        assert_eq!(matched_path, ["a"]);
        assert_eq!(value, None);
    }

    #[test]
    fn matches_exact_path() {
        let trie = create_trie();
        let (matched_path, value) = trie.find_closest_prefix(&["a", "b"]);
        assert_eq!(matched_path, ["a", "b"]);
        assert_eq!(value, Some(&true));
    }

    #[test]
    fn matches_wildcard_path() {
        let trie = create_trie();
        let (matched_path, value) = trie.find_closest_prefix(&["a", "b", "c"]);
        assert_eq!(matched_path, ["a", "b"]);
        assert_eq!(value, Some(&true));
    }

    #[test]
    fn removes_the_correct_item() {
        let mut trie = create_trie();
        trie.insert(&["a"], false);
        let remove_result = trie.remove(&["a", "b"]);
        assert_eq!(remove_result, Some(true));
        assert_eq!(trie.get(&["a"]), Some(&false));
        assert_eq!(trie.get(&["a", "b"]), None);
    }

    #[test]
    fn removes_the_correct_item_with_child() {
        let mut trie = create_trie();
        trie.insert(&["a"], false);
        trie.insert(&["a", "b", "c"], false);
        let remove_result = trie.remove(&["a", "b"]);
        assert_eq!(remove_result, Some(true));
        assert_eq!(trie.get(&["a"]), Some(&false));
        assert_eq!(trie.get(&["a", "b"]), None);
        assert_eq!(trie.get(&["a", "b", "c"]), Some(&false));
    }
}
