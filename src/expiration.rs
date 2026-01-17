use std::{
    sync::{Arc, Mutex},
    time::SystemTime,
};

#[cfg(feature = "ttl")]
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub(crate) struct ExpiringItem<T> {
    inner: Arc<Mutex<Option<T>>>,
    #[cfg(feature = "ttl")]
    background_task: Option<Arc<JoinHandle<()>>>,
}

impl<T: 'static + Send> ExpiringItem<T> {
    pub(crate) fn new(inner: T, expires_at: Option<SystemTime>) -> Self {
        let inner = Arc::new(Mutex::new(Some(inner)));

        #[cfg(feature = "ttl")]
        let background_task = {
            if let Some(expires_at) = expires_at {
                let inner = inner.clone();
                Some(Arc::new(tokio::task::spawn(async move {
                    let until_expiriation = expires_at.duration_since(SystemTime::now()).unwrap();
                    tokio::time::sleep(until_expiriation).await;
                    inner.lock().unwrap().take();
                })))
            } else {
                None
            }
        };

        #[cfg(not(feature = "ttl"))]
        if expires_at.is_some() {
            panic!("`ttl` feature is disabled");
        }

        Self {
            inner,
            #[cfg(feature = "ttl")]
            background_task,
        }
    }

    pub(crate) fn take(&self) -> Option<T> {
        let mut inner = self.inner.lock().unwrap();
        #[cfg(feature = "ttl")]
        if let Some(background_task) = &self.background_task {
            background_task.abort();
        }

        inner.take()
    }
}
