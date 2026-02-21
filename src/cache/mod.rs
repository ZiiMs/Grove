use std::time::Instant;
use tokio::sync::Mutex;

pub struct CachedData<T> {
    pub fetched_at: Instant,
    pub data: T,
}

pub struct Cache<T> {
    data: Mutex<Option<CachedData<T>>>,
    ttl_secs: u64,
}

impl<T: Clone + Send> Cache<T> {
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            data: Mutex::new(None),
            ttl_secs,
        }
    }

    pub async fn get(&self) -> Option<T> {
        let guard = self.data.lock().await;
        if let Some(ref cached) = *guard {
            if cached.fetched_at.elapsed().as_secs() < self.ttl_secs {
                return Some(cached.data.clone());
            }
        }
        None
    }

    pub async fn set(&self, data: T) {
        let mut guard = self.data.lock().await;
        *guard = Some(CachedData {
            fetched_at: Instant::now(),
            data,
        });
    }

    pub async fn invalidate(&self) {
        let mut guard = self.data.lock().await;
        *guard = None;
    }
}
