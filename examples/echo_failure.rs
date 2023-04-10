use async_trait::async_trait;
use maelstrom::protocol::{Message, MessageBody};
use maelstrom::{Node, Result, Runtime};
use serde_json::{Map, Value};
use simple_error::bail;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub(crate) fn main() -> Result<()> {
    Runtime::init(try_main())
}

async fn try_main() -> Result<()> {
    let handler = Arc::new(Handler::default());
    Runtime::new().with_handler(handler).run().await
}

#[derive(Clone, Default)]
struct Handler {
    inter: Arc<std::sync::atomic::AtomicI32>,
}

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, message: Message) -> Result<()> {
        match message.body.typo.as_str() {
            "echo" => {
                // runtime.spawn(received(runtime.clone(), self.clone(), message));
                received(runtime.clone(), self.clone(), message).await
            }
            _ => bail!("unknown message type: {}", message.body.typo),
        }
    }
}

async fn received(runtime: Runtime, handler: Handler, data: Message) -> Result<()> {
    if handler.inter.fetch_add(1, Ordering::SeqCst) > 0 {
        let body = MessageBody::from_str_error(1, "blah");
        return runtime.reply(data, body).await;
    }

    let echo = format!("Please echo {}", data.body.msg_id);
    let msg = Value::Object(Map::from_iter([("echo".to_string(), Value::String(echo))]));
    runtime.reply(data, msg).await
}
