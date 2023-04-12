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
- TODO: rpc / timeout
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

# API

## Responses

```rust
use async_trait::async_trait;
use maelstrom::protocol::{Message, MessageBody};
use maelstrom::{Node, Result, Runtime};
use serde_json::{Map, Value};
use std::sync::Arc;

#[derive(Clone, Default)]
struct Handler {}

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

        done(runtime, message)
    }
}

// Putting `#[serde(rename = "type")] typo: String` is not necessary,
// as it is auto-deducted.
#[derive(Serialize)]
struct EchoResponse {
    echo: String,
}

```

# Why

Because I am learning Rust and I liked Maelstrom and fly.io pretty much.
I wanted to play with different aspects of the language and ecosystem and
build an API that will be somewhat convenient and short. I am sorry for my ego.

Thanks Aphyr and guys a lot.

# Where

[GitHub](https://github.com/sitano/maelstrom-rust-node)

