//! Thread-safe key/value cache.

use std::collections::hash_map::{Entry, HashMap};
use std::hash::Hash;
use std::mem::needs_drop;
use std::ops::Deref;
use std::ptr::null;
use std::sync::{Arc, Mutex, RwLock};

/// Cache that remembers the result for each key.
#[derive(Debug)]
pub struct Cache<K, V> {
    inner: Mutex<HashMap<K, Arc<V>>>,
    called_map: Mutex<HashMap<K, bool>>,
}

impl<K, V> Default for Cache<K, V> {
    fn default() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            called_map: Mutex::new(HashMap::new()),
        }
    }
}

impl<K: Eq + Hash + Clone, V: Clone> Cache<K, V> {
    /// Retrieve the value or insert a new one created by `f`.
    ///
    /// An invocation to this function should not block another invocation with a different key. For
    /// example, if a thread calls `get_or_insert_with(key1, f1)` and another thread calls
    /// `get_or_insert_with(key2, f2)` (`key1≠key2`, `key1,key2∉cache`) concurrently, `f1` and `f2`
    /// should run concurrently.
    ///
    /// On the other hand, since `f` may consume a lot of resource (= money), it's undesirable to
    /// duplicate the work. That is, `f` should be run only once for each key. Specifically, even
    /// for concurrent invocations of `get_or_insert_with(key, f)`, `f` is called only once per key.
    ///
    /// Hint: the [`Entry`] API may be useful in implementing this function.
    ///
    /// [`Entry`]: https://doc.rust-lang.org/stable/std/collections/hash_map/struct.HashMap.html#method.entry
    pub fn get_or_insert_with<F: FnOnce(K) -> V>(&self, key: K, f: F) -> V {
        // implementation
        let mut inner = self.inner.lock().unwrap();

        match inner.entry(key.clone()) {
            // cache hit
            Entry::Occupied(entry) => {
                let arc_value = entry.get().clone();
                Arc::try_unwrap(arc_value).unwrap_or_else(|arc| (*arc).clone())
            }
            // cache miss
            Entry::Vacant(entry) => {
                let mut called = self.called_map.lock().unwrap();
                if called.contains_key(&key) {
                    // f is already called for key
                    drop(called);
                    drop(inner);
                    loop {
                        let mut inner = self.inner.lock().unwrap();
                        if inner.contains_key(&key) {
                            let arc_value = inner.get(&key).unwrap_or_else(|| panic!()).clone();
                            return Arc::try_unwrap(arc_value).unwrap_or_else(|arc| (*arc).clone());
                        }
                    }
                }
                called.insert(key.clone(), true);
                drop(called);
                drop(inner);

                let value = f(key.clone());
                let arc_value = Arc::new(value.clone());

                inner = self.inner.lock().unwrap();
                inner.insert(key, arc_value);
                value
            }
        }
    }
}
