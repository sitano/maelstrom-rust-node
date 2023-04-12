use async_trait::async_trait;
use log::info;
use maelstrom::protocol::Message;
use maelstrom::{done, Node, Result, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub(crate) fn main() -> Result<()> {
    Runtime::init(try_main())
}

async fn try_main() -> Result<()> {
    let handler = Arc::new(Handler::default());
    Runtime::new().with_handler(handler).run().await
}

#[derive(Clone, Default)]
struct Handler {
    inner: Arc<Mutex<Vec<u64>>>,
}

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
        match req.get_type() {
            "read" => {
                let data = self.inner.lock().unwrap().clone();
                let msg = ReadResponse { messages: data };
                return runtime.reply(req, msg).await;
            }
            "broadcast" => {
                let raw = Value::Object(req.body.extra.clone());

                let mut msg = serde_json::from_value::<BroadcastRequest>(raw)?;
                msg.typ = req.body.typ.clone();

                self.inner.lock().unwrap().push(msg.message);

                if !runtime.nodes().contains(&req.src) {
                    for node in runtime.nodes().to_vec() {
                        if node != runtime.node_id() {
                            runtime.spawn(Runtime::rpc(runtime.clone(), node.clone(), msg.clone()));
                        }
                    }
                }

                return runtime.reply_ok(req).await;
            }
            "topology" => {
                info!("new topology {:?}", req.body.extra.get("topology").unwrap());
                return runtime.reply_ok(req).await;
            }
            _ => done(runtime, req),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct BroadcastRequest {
    #[serde(default, rename = "type")]
    typ: String,
    message: u64,
}

// #[derive(Deserialize)]
// struct TopologyRequest {
//     topology: HashMap<String, Vec<String>>,
// }

#[derive(Serialize)]
struct ReadResponse {
    messages: Vec<u64>,
}
