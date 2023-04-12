# maelstrom-rust-demo

Yet another one Rust crate for implementing nodes for https://github.com/jepsen-io/maelstrom and solve
https://fly.io/dist-sys/ challenges.

# What is Maelstrom?

Maelstrom is a platform for learning distributed systems. It is build around Jepsen and Elle to ensure no properties are
violated. With maelstrom you build nodes that form distributed system that can process different workloads.

# Features

- async (tokio)
- multi-threading
- simple API - single trait fn to implement
- response types auto-deduction, extra data available via Value()
- unknown message types handling
- rpc + timeout
- TODO: kv

# Examples

## Echo server

```bash
$ cargo build --examples
$ maelstrom test -w echo --bin ./target/debug/examples/echo --node-count 1 --time-limit 10 --log-stderr
````

implementation:

```rust
use async_trait::async_trait;
use maelstrom::protocol::Message;
use maelstrom::{Node, Result, Runtime};
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
            let echo = req.body.clone().with_type("echo_ok");
            return runtime.reply(req, echo).await;
        }

        done(runtime, message)
    }
}
```

spec:

receiving

    {
      "src": "c1",
      "dest": "n1",
      "body": {
        "type": "echo",
        "msg_id": 1,
        "echo": "Please echo 35"
      }
    }

send back the same msg with body.type == echo_ok.

    {
      "src": "n1",
      "dest": "c1",
      "body": {
        "type": "echo_ok",
        "msg_id": 1,
        "in_reply_to": 1,
        "echo": "Please echo 35"
      }
    }

## Broadcast

```bash
$ cargo build --examples
$ RUST_LOG=debug maelstrom test -w broadcast --bin ./target/debug/examples/broadcast --node-count 2 --time-limit 20 --rate 10 --log-stderr
````


# API

## RPC

```rust
use async_trait::async_trait;
use maelstrom::protocol::Message;
use maelstrom::{Node, Result, Runtime};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Handler {}

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
        // 1.
        runtime.spawn(Runtime::rpc(runtime.clone(), node.clone(), msg.clone()));
        
        // 2. put it into runtime.spawn(async move { ... }) if needed
        let res: RPCResult = Runtime::rpc(runtime.clone(), node.clone(), msg.clone()).await?;
        let _msg: Result<Message> = res.await;
        
        // 3. put it into runtime.spawn(async move { ... }) if needed
        let mut res: RPCResult = Runtime::rpc(runtime.clone(), node.clone(), msg.clone()).await?;
        let (mut ctx, _handler) = Context::with_timeout(Duration::from_secs(1));
        let _msg: Message = res.done_with(ctx).await?;
            
        return runtime.reply_ok(req).await;
    }
}
```

## Responses

```rust
use async_trait::async_trait;
use log::info;
use maelstrom::protocol::Message;
use maelstrom::{done, Node, Result, Runtime};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct Handler { /* ... */ }

#[async_trait]
impl Node for Handler {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
        if req.get_type() == "echo" {
            let echo = req.body.clone().with_type("echo_ok");
            return runtime.reply(req, echo).await;
        }

        if req.get_type() == "echo" {
            let echo = format!("Another echo {}", message.body.msg_id);
            let msg = Value::Object(Map::from_iter([("echo".to_string(), Value::String(echo))]));
            return runtime.reply(message, msg).await;
        }

        if req.get_type() == "echo" {
            let err = maelstrom::Error::TemporarilyUnavailable {};
            let body = ErrorMessageBody::from_error(err);
            return runtime.reply(message, body).await;
        }

        if req.get_type() == "echo" {
            let body = MessageBody::default().with_type("echo_ok").with_reply_to(req.body.msg_id);
            // send: no response type auto-deduction and no reply_to
            return runtime.send(message, body).await;
        }

        if req.get_type() == "echo" {
            return runtime.reply(message, EchoResponse { echo: "blah".to_string() }).await;
        }

        if req.get_type() == "read" {
            let data = self.inner.lock().unwrap().clone();
            let msg = ReadResponse { messages: data };
            return runtime.reply(req, msg).await;
        }

        if req.get_type() == "broadcast" {
            let raw = Value::Object(req.body.extra.clone());

            let mut msg = serde_json::from_value::<BroadcastRequest>(raw)?;
            msg.typ = req.body.typ.clone();
            
            return runtime.reply_ok(req).await;
        }

        if req.get_type() == "topology" {
            info!("new topology {:?}", req.body.extra.get("topology").unwrap());
            return runtime.reply_ok(req).await;
        }

        done(runtime, message)
    }
}

// Putting `#[serde(rename = "type")] typo: String` is not necessary,
// as it is auto-deducted.
#[derive(Serialize)]
struct EchoResponse {
    echo: String,
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
```

# Why

Because I am learning Rust and I liked Maelstrom and fly.io pretty much.
I wanted to play with different aspects of the language and ecosystem and
build an API that will be somewhat convenient and short. I am sorry for my ego.

Thanks Aphyr and guys a lot.

# Where

[GitHub](https://github.com/sitano/maelstrom-rust-node)

