#![feature(async_closure)]

mod waitgroup;

use crate::waitgroup::WaitGroup;
use log::{debug, info};
use std::future::Future;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::io::{stdin, stdout, BufReader};
use tokio::io::{AsyncBufReadExt, Stdout};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

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
        tokio::spawn(async move {
            let res = future.await;
            drop(h);
            res
        })
    }
}

pub(crate) fn main() -> Result<()> {
    env_logger::init();
    debug!("inited");

    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    runtime.block_on(try_main())
}

async fn try_main() -> Result<()> {
    let stdin = BufReader::new(stdin());
    let runtime = Arc::new(Runtime::new(stdout()));
    let mut lines_from_stdin = stdin.lines();
    while let Some(line) = lines_from_stdin.next_line().await? {
        info!("Received {}", line);
        runtime.spawn(received(runtime.clone(), line));
    }
    Ok(runtime.serving.wait().await)
}

async fn received(runtime: Arc<Runtime>, data: String) -> Result<()> {
    runtime.send_raw(data.as_str()).await
}
