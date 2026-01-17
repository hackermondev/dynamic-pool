use crossbeam_queue::ArrayQueue;
use dashmap::DashMap;
use std::fmt::{Debug, Formatter};
use std::hash::Hash;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Weak};
use std::time::{Duration, SystemTime};

use crate::expiration::ExpiringItem;
use crate::DynamicReset;

#[derive(Debug, Default)]
pub struct DynamicPoolConfig {
    pub max_capacity: usize,
    pub time_to_live: Option<Duration>,
}

impl DynamicPoolConfig {
    pub fn with_max_capacity(mut self, max_capacity: usize) -> Self {
        self.max_capacity = max_capacity;
        self
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.time_to_live = Some(ttl);
        self
    }
}

#[derive(Debug)]
pub struct DynamicPool<K: Eq + Hash, T: DynamicReset> {
    inner: Arc<DashMap<K, Arc<PoolData<T>>>>,
    config: Arc<DynamicPoolConfig>,
}

impl<K: Eq + Hash, T: DynamicReset + 'static + Send> DynamicPool<K, T> {
    /// creates a new `DynamicPool<T>`.
    pub fn new(config: DynamicPoolConfig) -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
            config: Arc::new(config),
        }
    }

    pub fn insert(&self, k: K, item: T) -> Result<(), T> {
        let pool = self.inner.entry(k).or_insert_with(|| {
            Arc::new(PoolData {
                items: ArrayQueue::new(self.config.max_capacity),
            })
        });

        let expiration = self
            .config
            .time_to_live
            .as_ref()
            .map(|ttl| SystemTime::now() + *ttl);
        let item = ExpiringItem::new(item, expiration);
        pool.items.push(item).map_err(|e| e.0.take().unwrap())
    }

    /// attempts to take an item from a pool, returning `none` if none is available. will never allocate.
    pub fn try_take(&self, k: &K) -> Option<DynamicPoolItem<T>> {
        let pool = self.inner.get(k)?;
        loop {
            let object = pool.items.pop().ok()?;
            let object = object.take();
            if object.is_none() {
                continue;
            }

            let object = object.unwrap();
            let data = Arc::downgrade(&pool);
            return Some(DynamicPoolItem {
                data,
                object: Some(object),
                config: self.config.clone(),
            });
        }
    }

    /// returns the number of objects currently in use in a pool. does not include objects that have been detached.
    #[inline]
    pub fn used(&self, k: &K) -> usize {
        let pool = self.inner.get(k);
        if pool.is_none() {
            return 0;
        }

        let pool = pool.unwrap();
        Arc::weak_count(&pool)
    }

    #[inline]
    pub fn capacity(&self, k: &K) -> usize {
        let pool = self.inner.get(k);
        if pool.is_none() {
            return 0;
        }

        let pool = pool.unwrap();
        pool.items.capacity()
    }
}

impl<K: Eq + Hash + Clone, T: DynamicReset> Clone for DynamicPool<K, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            config: self.config.clone(),
        }
    }
}

// data shared by a `DynamicPool`.
struct PoolData<T> {
    items: ArrayQueue<ExpiringItem<T>>,
}

impl<T: DynamicReset + Debug> Debug for PoolData<T> {
    fn fmt(&self, formatter: &mut Formatter) -> Result<(), std::fmt::Error> {
        formatter
            .debug_struct("PoolData")
            .field("items", &self.items)
            .finish()
    }
}

/// an object, checked out from a dynamic pool object.
#[derive(Debug)]
pub struct DynamicPoolItem<T: DynamicReset + 'static + Send> {
    data: Weak<PoolData<T>>,
    object: Option<T>,
    config: Arc<DynamicPoolConfig>,
}

impl<T: DynamicReset + 'static + Send> DynamicPoolItem<T> {
    /// detaches this instance from the pool, returns T.
    pub fn detach(mut self) -> T {
        self.object
            .take()
            .expect("invariant: object is always `some`.")
    }
}

impl<T: DynamicReset + 'static + Send> AsRef<T> for DynamicPoolItem<T> {
    fn as_ref(&self) -> &T {
        self.object
            .as_ref()
            .expect("invariant: object is always `some`.")
    }
}

impl<T: DynamicReset + 'static + Send> Deref for DynamicPoolItem<T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.object
            .as_ref()
            .expect("invariant: object is always `some`.")
    }
}

impl<T: DynamicReset + 'static + Send> DerefMut for DynamicPoolItem<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.object
            .as_mut()
            .expect("invariant: object is always `some`.")
    }
}

impl<T: DynamicReset + 'static + Send> Drop for DynamicPoolItem<T> {
    fn drop(&mut self) {
        if let Some(mut object) = self.object.take() {
            object.reset();
            if let Some(pool) = self.data.upgrade() {
                let expiration = self
                    .config
                    .time_to_live
                    .as_ref()
                    .map(|ttl| SystemTime::now() + *ttl);
                let object = ExpiringItem::new(object, expiration);
                pool.items.push(object).ok();
            }
        }
    }
}
