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
/// # Examples
///
/// ```
/// use std::thread;
///
/// // Create a new wait group.
/// let wg = WaitGroup::new();
///
/// for _ in 0..4 {
///     // Create another reference to the wait group.
///     let wg = wg.clone();
///
///     thread::spawn(move || {
///         // Do some work.
///
///         // Drop the reference to the wait group.
///         drop(wg);
///     });
/// }
///
/// // Block until all threads have finished their work.
/// wg.wait();
/// # std::thread::sleep(std::time::Duration::from_millis(500)); // wait for background threads closed: https://github.com/rust-lang/miri/issues/1371
/// ```
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
    ///
    /// # Examples
    ///
    /// ```
    /// let wg = WaitGroup::new();
    /// ```
    pub fn new() -> Self {
        Self::default()
    }

    /// Drops this reference and waits until all other references are dropped.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::thread;
    ///
    /// let wg = WaitGroup::new();
    ///
    /// thread::spawn({
    ///     let wg = wg.clone();
    ///     move || {
    ///         // Block until both threads have reached `wait()`.
    ///         wg.wait();
    ///     }
    /// });
    ///
    /// // Block until both threads have reached `wait()`.
    /// wg.wait();
    /// # std::thread::sleep(std::time::Duration::from_millis(500)); // wait for background threads closed: https://github.com/rust-lang/miri/issues/1371
    /// ```
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
        let count: &usize = &*self.inner.count.lock().unwrap();
        f.debug_struct("WaitGroup").field("count", count).finish()
    }
}
