use crate::WaitGroup;
use futures::FutureExt;
use log::info;
use std::future::Future;
use tokio::io::{AsyncWriteExt, Stdout};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Runtime {
    pub out: Mutex<Stdout>,
    pub serving: WaitGroup,
}

impl Runtime {
    pub fn new(out: Stdout) -> Self {
        return Runtime {
            out: Mutex::new(out),
            serving: WaitGroup::new(),
        };
    }

    pub async fn send_raw(self: &Self, msg: &str) -> Result<()> {
        {
            let mut out = self.out.lock().await;
            out.write_all(msg.as_bytes()).await?;
            out.write_all(b"\n").await?;
        }
        info!("Sent {}", msg);
        Ok(())
    }

    #[track_caller]
    pub fn spawn<T>(self: &Self, future: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let h = self.serving.clone();
        tokio::spawn(future.then(async move |x| {
            drop(h);
            x
        }))
    }
}
