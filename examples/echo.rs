use async_trait::async_trait;
use maelstrom::protocol::Message;
use maelstrom::{Node, Result, Runtime};
use serde::Serialize;
use std::sync::Arc;

pub(crate) fn main() -> Result<()> {
    Runtime::init(try_main())
}

async fn try_main() -> Result<()> {
    let handler = Arc::new(Handler::default());
    Runtime::new().with_handler(handler).run().await
}

#[derive(Clone, Default)]
struct Handler {}

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
        if req.get_type() == "echo" {
            let echo = format!("Please echo {}", req.body.msg_id);
            return runtime.reply(req, EchoResponse { echo }).await;
        }

        Ok(())
    }
}

/// Putting `#[serde(rename = "type")] typo: String` is not necessary,
/// as it is auto-deducted.
#[derive(Serialize)]
struct EchoResponse {
    echo: String,
}
