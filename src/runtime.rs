#![allow(dead_code)]

use crate::protocol::{Message, MessageBody};
use crate::waitgroup::WaitGroup;
use async_trait::async_trait;
use futures::FutureExt;
use log::{debug, error, info};
use serde::Serialize;
use serde_json::Value;
use simple_error::bail;
use std::future::Future;
use std::sync::Arc;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader, Stdout};
use tokio::select;
use tokio::sync::{mpsc, Mutex, OnceCell};
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

    handler: OnceCell<Arc<dyn Node>>,

    out: Mutex<Stdout>,

    serving: WaitGroup,
}

// Handler is the trait that implements message handling.
#[async_trait]
pub trait Node: Sync + Send {
    async fn process(self: &Self, runtime: Runtime, request: Message) -> Result<()>;
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
        return self.run_with(BufReader::new(stdin())).await;
    }

    pub async fn run_with<R>(self: &Self, input: BufReader<R>) -> Result<()>
    where
        R: AsyncRead + Unpin,
    {
        let stdin = input;

        let (tx_err, mut rx_err) = mpsc::channel::<Result<()>>(1);
        let mut tx_out: Result<()> = Ok(());

        let mut lines_from_stdin = stdin.lines();
        loop {
            select! {
                Ok(read) = lines_from_stdin.next_line().fuse() => {
                    match read {
                        Some(line) =>{
                            if line.trim().is_empty() {
                                continue;
                            }

                            info!("Received {}", line);

                            // TODO: error message to inc line
                            let msg = serde_json::from_str::<Message>(line.as_str())?;
                            // TODO: msg.body.raw set
                            self.spawn(Self::process_request(self.clone(), msg, tx_err.clone()));
                            // TODO: not init-ed
                        }
                        None => break
                    }
                },
                Some(e) = rx_err.recv() => {
                    tx_out = e;
                    rx_err.close();
                    break
                },
                else => break
            }
        }

        select! {
            _ = self.done() => {},
            Some(e) = rx_err.recv() => {
                tx_out = e;
                rx_err.close();
            }
        }

        if let Err(e) = tx_out {
            debug!("node error: {}", e);
            return Err(e);
        }

        // TODO: print stats?
        debug!("node done");

        Ok(())
    }

    async fn process_request(runtime: Runtime, req: Message, tx_err: mpsc::Sender<Result<()>>) {
        // TODO: handle init message
        if let Some(handler) = runtime.inter.handler.get() {
            let res = handler.process(runtime.clone(), req).await;
            if let Err(e) = res {
                error!("process_request error: {}", e);
                tx_err.send(Err(e)).await.unwrap();
            }
        }
    }
}

impl Clone for Runtime {
    fn clone(&self) -> Self {
        return Runtime {
            inter: self.inter.clone(),
        };
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub struct BlackHoleNode {}

#[async_trait]
impl Node for BlackHoleNode {
    async fn process(self: &Self, _: Runtime, _: Message) -> Result<()> {
        Ok(())
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub struct IOFailingNode {}

#[async_trait]
impl Node for IOFailingNode {
    async fn process(self: &Self, _: Runtime, _: Message) -> Result<()> {
        bail!("IOFailingNode: process failed")
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub struct EchoNode {}

#[async_trait]
impl Node for EchoNode {
    async fn process(self: &Self, runtime: Runtime, req: Message) -> Result<()> {
        let resp = Value::Object(serde_json::Map::default());
        runtime.reply(req, resp).await
    }
}

#[cfg(test)]
mod test {
    use crate::{MembershipState, Result, Runtime};
    use log::debug;
    use tokio::io::BufReader;
    use tokio_util::sync::CancellationToken;

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

    #[tokio::test]
    async fn io_failure() {
        let handler = std::sync::Arc::new(crate::IOFailingNode::default());
        let runtime = Runtime::new().with_handler(handler);
        let cursor = std::io::Cursor::new(
            r#"
            
            {"src":"c0","dest":"n0","body":{"type":"echo","msg_id":1}}
            "#,
        );
        let token = CancellationToken::new();
        runtime.spawn(async move { token.cancelled().await });
        let run = runtime.run_with(BufReader::new(cursor));
        assert!(matches!(run.await, Err(_)));
    }
}
