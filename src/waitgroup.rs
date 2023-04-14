/// Derived from crossbeam-utils.
use std::fmt;
use std::sync::{Arc, Mutex};
use tokio::sync::Notify;

/// Enables threads to synchronize the beginning or end of some computation.
///
/// # Wait groups vs barriers
///
/// `WaitGroup` is very similar to [`Barrier`], but there are a few differences:
///
/// * [`Barrier`] needs to know the number of threads at construction, while `WaitGroup` is cloned to
///   register more threads.
///
/// * A [`Barrier`] can be reused even after all threads have synchronized, while a `WaitGroup`
///   synchronizes threads only once.
///
/// * All threads wait for others to reach the [`Barrier`]. With `WaitGroup`, each thread can choose
///   to either wait for other threads or to continue without blocking.
///
/// [`Barrier`]: std::sync::Barrier
pub struct WaitGroup {
    inner: Arc<Inner>,
}

/// Inner state of a `WaitGroup`.
struct Inner {
    cvar: Notify,
    count: Mutex<usize>,
}

impl Default for WaitGroup {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                cvar: Notify::new(),
                count: Mutex::new(1),
            }),
        }
    }
}

impl WaitGroup {
    /// Creates a new wait group and returns the single reference to it.
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops this reference and waits until all other references are dropped.
    pub async fn wait(&self) {
        let future = self.inner.cvar.notified();

        if *self.inner.count.lock().unwrap() == 1 {
            return;
        }

        future.await;
    }
}

impl Drop for WaitGroup {
    fn drop(&mut self) {
        let mut count = self.inner.count.lock().unwrap();
        *count -= 1;

        if *count == 1 {
            self.inner.cvar.notify_waiters();
        }
    }
}

impl Clone for WaitGroup {
    fn clone(&self) -> WaitGroup {
        let mut count = self.inner.count.lock().unwrap();
        *count += 1;

        WaitGroup {
            inner: self.inner.clone(),
        }
    }
}

impl fmt::Debug for WaitGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count: &usize = &self.inner.count.lock().unwrap();
        f.debug_struct("WaitGroup").field("count", count).finish()
    }
}
