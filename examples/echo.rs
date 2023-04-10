#![allow(incomplete_features)]
#![feature(async_fn_in_trait)]

use maelstrom::protocol::Message;
use maelstrom::{Node, Result, Runtime};
use serde::Serialize;
use simple_error::bail;
use std::sync::Arc;

pub(crate) fn main() -> Result<()> {
    maelstrom::init(try_main())
}

async fn try_main() -> Result<()> {
    let handler = Arc::new(Handler::default());
    Runtime::new().with_handler(handler).run().await
}

#[derive(Clone, Default)]
struct Handler {}

impl Node<Handler> for Handler {
    async fn process(&self, runtime: Runtime<Handler>, req: Message) -> Result<()> {
        match req.body.typo.as_str() {
            "echo" => {
                let echo = format!("Please echo {}", req.body.msg_id);
                let resp = EchoResponse { echo };
                runtime.reply(req, resp).await
            }
            _ => bail!("unknown message type: {}", req.body.typo),
        }
    }
}

async fn received(runtime: Runtime<Handler>, data: Message) -> Result<()> {}

/// Putting `#[serde(rename = "type")] typo: String` is not necessary,
/// as it is auto-deducted.
#[derive(Serialize)]
struct EchoResponse {
    echo: String,
}
