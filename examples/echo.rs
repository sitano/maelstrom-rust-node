use log::debug;
use maelstrom::protocol::Message;
use maelstrom::{log_util, Result};
use maelstrom::{Node, Runtime};
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
struct Handler {}

impl Node for Handler {
    fn process(&self, runtime: Runtime, message: Message) -> Result<()> {
        match message.body.typo.as_str() {
            "echo" => {
                runtime.spawn(received(runtime.clone(), message));
                Ok(())
            }
            _ => bail!("unknown message type: {}", message.body.typo),
        }
    }
}

async fn received(runtime: Runtime, data: Message) -> Result<()> {
    let resp = EchoResponse {
        typo: "echo_ok".to_string(),
        echo: "a".to_string(),
    };

    runtime.reply(data, resp).await
}

#[derive(Serialize)]
struct EchoResponse {
    #[serde(rename = "type")]
    typo: String,
    echo: String,
}
