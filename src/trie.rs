use std::collections::HashMap;

struct TrieNode<V: Clone> {
    value: Option<V>,
    children: HashMap<String, Self>,
}

pub struct Trie<V: Clone> {
    root: TrieNode<V>,
}

impl<V: Clone> Trie<V> {
    pub fn new() -> Self {
        Self {
            root: TrieNode {
                value: None,
                children: HashMap::new(),
            },
        }
    }

    pub fn insert<I, S>(&mut self, path: I, value: V)
    where
        I: IntoIterator<Item = S>,
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

    pub fn get_mut<I, S>(&mut self, path: I) -> Option<&mut V>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut node = &mut self.root;

        for key in path {
            node = node.children.get_mut(key.as_ref())?;
        }

        node.value.as_mut()
    }

    pub fn get<I, S>(&self, path: I) -> Option<&V>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut node = &self.root;

        for key in path {
            node = node.children.get(key.as_ref())?;
        }

        node.value.as_ref()
    }

    pub fn find_closest_prefix<I, S>(&self, path: I) -> (Vec<String>, Option<&V>)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str> + ToString,
    {
        let mut node_path = Vec::new();
        let mut node = &self.root;

        for key in path {
            if let Some(child) = node.children.get(key.as_ref()) {
                node_path.push(key.to_string());
                node = child;
            } else {
                break;
            }
        }

        (node_path, node.value.as_ref())
    }
}

impl<V: Clone> Default for Trie<V> {
    fn default() -> Self {
        Self::new()
    }
}
