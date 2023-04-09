#![feature(async_closure)]

mod log_util;
mod protocol;
mod runtime;
mod waitgroup;

use crate::protocol::{Message, MessageBody};
use crate::runtime::Result;
use crate::runtime::Runtime;
use crate::waitgroup::WaitGroup;
use log::debug;
use serde::Deserialize;
use serde::Serialize;
use simple_error::bail;
use std::sync::Arc;

pub(crate) fn main() -> Result<()> {
    log_util::builder().init();
    debug!("inited");

    let runtime = tokio::runtime::Runtime::new()?;
    let _guard = runtime.enter();

    runtime.block_on(try_main())
}

async fn try_main() -> Result<()> {
    let handler = Arc::new(Handler::default());
    Runtime::new().with_handler(handler).run().await
}

#[derive(Clone, Default)]
struct Handler {
    inter: Arc<std::sync::atomic::AtomicI32>,
}

impl runtime::Handler for Handler {
    fn process(&self, runtime: Runtime, message: Message) -> Result<()> {
        match message.body.typo.as_str() {
            "echo" => {
                runtime.spawn(received(runtime.clone(), self.clone(), message));
                Ok(())
            }
            _ => bail!("unknown message type: {}", message.body.typo),
        }
    }
}

async fn received(runtime: Runtime, handler: Handler, data: Message) -> Result<()> {
    let i = handler
        .inter
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if i >= 10 {
        return runtime
            .send(data, MessageBody::from_str_error(1, "blah"))
            .await;
    }
    runtime
        .reply(
            data,
            EchoResponse {
                typo: "echo_ok".to_string(),
                echo: "a".to_string(),
            },
        )
        .await
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Default)]
struct EchoResponse {
    #[serde(rename = "type")]
    typo: String,
    echo: String,
}
