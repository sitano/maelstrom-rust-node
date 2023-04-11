# maelstrom-rust-demo

Rust crate to implement https://github.com/jepsen-io/maelstrom / fly.io challenges.

# Features

- async (tokio)
- multi-threading

# Examples

## Echo server

    $ cargo build --examples
    $ maelstrom test -w echo --bin ./target/debug/examples/echo --node-count 1 --time-limit 10 --log-stderr

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
            let echo = req.body.extra.clone();
            return runtime.reply(req, echo).await;
        }

        Ok(())
    }
}
```
