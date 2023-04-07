#![feature(async_closure)]

mod log_util;
mod protocol;
mod runtime;
mod waitgroup;

use crate::runtime::Result;
use crate::runtime::Runtime;
use crate::waitgroup::WaitGroup;
use log::{debug, info};
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::{stdin, stdout, BufReader};

pub(crate) fn main() -> Result<()> {
    log_util::builder().init();
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

    runtime.done().await;

    // TODO: print stats?
    debug!("done");

    Ok(())
}

async fn received(runtime: Arc<Runtime>, data: String) -> Result<()> {
    runtime.send_raw(data.as_str()).await
}
