use async_trait::async_trait;
use maelstrom::protocol::{Message, MessageBody};
use maelstrom::{Node, Result, Runtime};
use serde_json::{Map, Value};
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
        if message.get_type() == "echo" {
            if self.inter.fetch_add(1, Ordering::SeqCst) > 0 {
                let body = MessageBody::from_str_error(1, "blah");
                return runtime.reply(message, body).await;
            }

            let echo = format!("Please echo {}", message.body.msg_id);
            let msg = Value::Object(Map::from_iter([("echo".to_string(), Value::String(echo))]));
            return runtime.reply(message, msg).await;
        }

        Ok(())
    }
}
