#![allow(dead_code)]

use crate::protocol::{Message, MessageBody};
use crate::waitgroup::WaitGroup;
use futures::FutureExt;
use log::{debug, info};
use serde::Serialize;
use serde_json::Value;
use simple_error::bail;
use std::future::Future;
use std::sync::Arc;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncWriteExt, BufReader, Stdout};
use tokio::sync::{Mutex, OnceCell};
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Runtime {
    // we need an arc<> here to be able to pass runtime.clone() from &Self run() further
    // to the handler.
    inter: Arc<Inter>,
}

struct Inter {
    // OnceCell seems works better here than RwLock, but what do we think of cluster membership change?
    // How the API should behave if the Maelstrom will send the second init message?
    // Let's pick the approach when it is possible to only once initialize a node and a cluster
    // membership change must start a new node and stop the old ones.
    membership: OnceCell<MembershipState>,

    handler: OnceCell<Arc<dyn Node + Sync + Send>>,

    out: Mutex<Stdout>,

    serving: WaitGroup,
}

// Handler is the trait that implements message handling.
pub trait Node {
    // TODO: can we have async process instead?
    fn process(self: &Self, runtime: Runtime, message: Message) -> Result<()>;
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct MembershipState {
    pub node_id: String,
    pub nodes: Vec<String>,
}

impl Runtime {
    pub fn init<F: Future>(future: F) -> F::Output {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _guard = runtime.enter();

        crate::log::builder().init();
        debug!("inited");

        runtime.block_on(future)
    }
}

impl Runtime {
    pub fn new() -> Self {
        return Runtime {
            inter: Arc::new(Inter {
                membership: OnceCell::new(),
                handler: OnceCell::new(),
                out: Mutex::new(stdout()),
                serving: WaitGroup::new(),
            }),
        };
    }

    pub fn with_handler(self, handler: Arc<dyn Node + Send + Sync>) -> Self {
        if let Err(_) = self.inter.handler.set(handler) {
            panic!("runtime handler is already initialized");
        }
        return self;
    }

    pub async fn send_raw(self: &Self, msg: &str) -> Result<()> {
        {
            let mut out = self.inter.out.lock().await;
            out.write_all(msg.as_bytes()).await?;
            out.write_all(b"\n").await?;
        }
        info!("Sent {}", msg);
        Ok(())
    }

    pub async fn send<T>(self: &Self, req: Message, resp: T) -> Result<()>
    where
        T: Serialize,
    {
        let extra = match serde_json::to_value(resp) {
            Ok(v) => match v {
                Value::Object(m) => m,
                _ => bail!("response object has invalid serde_json::Value kind"),
            },
            Err(e) => bail!("response object is invalid, can't convert: {}", e),
        };

        // TODO: msg id

        let msg = Message {
            src: req.dest,
            dest: req.src,
            body: MessageBody::from_extra(extra),
        };

        let answer = serde_json::to_string(&msg)?;
        return self.send_raw(answer.as_str()).await;
    }

    pub async fn reply<T>(self: &Self, req: Message, resp: T) -> Result<()>
    where
        T: Serialize,
    {
        let mut extra = match serde_json::to_value(resp) {
            Ok(v) => match v {
                Value::Object(m) => m,
                _ => bail!("response object has invalid serde_json::Value kind"),
            },
            Err(e) => bail!("response object is invalid, can't convert: {}", e),
        };

        if !extra.contains_key("type") && !req.body.typo.is_empty() {
            extra.insert("type".to_string(), Value::String(req.body.typo + "_ok"));
        }

        // TODO: if extra type is empty, use req.body.typ + _ok
        // TODO: msg id

        let msg = Message {
            src: req.dest,
            dest: req.src,
            body: MessageBody::from_extra(extra).with_reply_to(req.body.msg_id),
        };

        let answer = serde_json::to_string(&msg)?;
        return self.send_raw(answer.as_str()).await;
    }

    #[track_caller]
    pub fn spawn<T>(self: &Self, future: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let h = self.inter.serving.clone();
        tokio::spawn(future.then(async move |x| {
            drop(h);
            x
        }))
    }

    pub fn node_id(self: &Self) -> &str {
        if let Some(v) = self.inter.membership.get() {
            return v.node_id.as_str();
        }
        return &"";
    }

    pub fn nodes(self: &Self) -> &[String] {
        if let Some(v) = self.inter.membership.get() {
            return v.nodes.as_slice();
        }
        return &[];
    }

    pub fn set_membership_state(self: &Self, state: MembershipState) -> Result<()> {
        if let Err(e) = self.inter.membership.set(state) {
            bail!("membership is inited: {}", e);
        }
        Ok(())
    }

    pub async fn done(self: &Self) {
        self.inter.serving.wait().await;
    }

    pub async fn run(self: &Self) -> Result<()> {
        let stdin = BufReader::new(stdin());

        let mut lines_from_stdin = stdin.lines();
        while let Some(line) = lines_from_stdin.next_line().await? {
            if line.is_empty() {
                continue;
            }

            info!("Received {}", line);

            // TODO: error message to inc line
            let msg = serde_json::from_str::<Message>(line.as_str())?;
            // TODO: msg.body.raw set
            if let Some(handler) = self.inter.handler.get() {
                handler.process(self.clone(), msg)?;
                // TODO: exit on error
            }
            // TODO: not init-ed
        }

        self.done().await;

        // TODO: print stats?
        debug!("done");

        Ok(())
    }
}

impl Clone for Runtime {
    fn clone(&self) -> Self {
        return Runtime {
            inter: self.inter.clone(),
        };
    }
}

#[cfg(test)]
mod test {
    use crate::{MembershipState, Result, Runtime};
    use log::debug;

    #[test]
    fn membership() -> Result<()> {
        let tokio_runtime = tokio::runtime::Runtime::new()?;
        tokio_runtime.block_on(async move {
            // TODO: use fake stdout
            let runtime = Runtime::new();
            let runtime0 = runtime.clone();
            let s1 = MembershipState::example("n0", &["n0", "n1"]);
            let s2 = MembershipState::example("n1", &["n0", "n1"]);
            runtime.spawn(async move {
                runtime0.set_membership_state(s1).unwrap();
                debug!("{}", runtime0.node_id());
                async move {
                    assert!(matches!(runtime0.set_membership_state(s2), Err(_)));
                }
                .await;
            });
            runtime.done().await;
            assert_eq!(
                runtime.node_id(),
                "n0",
                "invalid node id, can't be anything else"
            );
        });
        Ok(())
    }

    impl MembershipState {
        fn example(n: &str, s: &[&str]) -> Self {
            return MembershipState {
                node_id: n.to_string(),
                nodes: s.iter().map(|x| x.to_string()).collect(),
            };
        }
    }
}
