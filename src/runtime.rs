#![allow(dead_code)]

use crate::error::Error;
use crate::protocol::{ErrorMessageBody, InitMessageBody, Message, MessageBody};
use crate::waitgroup::WaitGroup;
use crate::RPCResult;
use async_trait::async_trait;
use futures::FutureExt;
use log::{debug, error, info, warn};
use serde::Serialize;
use serde_json::Value;
use simple_error::bail;
use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::{AcqRel, Release};
use std::sync::Arc;
use tokio::io::{stdin, stdout, AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader, Stdout};
use tokio::select;
use tokio::sync::oneshot::Sender;
use tokio::sync::{mpsc, oneshot, Mutex, OnceCell};
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Runtime {
    // we need an arc<> here to be able to pass runtime.clone() from &Self run() further
    // to the handler.
    inter: Arc<Inter>,
}

struct Inter {
    msg_id: AtomicU64,

    // OnceCell seems works better here than RwLock, but what do we think of cluster membership change?
    // How the API should behave if the Maelstrom will send the second init message?
    // Let's pick the approach when it is possible to only once initialize a node and a cluster
    // membership change must start a new node and stop the old ones.
    membership: OnceCell<MembershipState>,

    handler: OnceCell<Arc<dyn Node>>,

    rpc: Mutex<HashMap<u64, Sender<Message>>>,

    out: Mutex<Stdout>,

    serving: WaitGroup,
}

// Handler is the trait that implements message handling.
#[async_trait]
pub trait Node: Sync + Send {
    /// Main handler function that processes incoming requests.
    ///
    /// Example:
    ///
    /// ```
    /// use async_trait::async_trait;
    /// use maelstrom::protocol::Message;
    /// use maelstrom::{Node, Result, Runtime, done};
    ///
    /// struct Handler {}
    ///
    /// #[async_trait]
    /// impl Node for Handler {
    ///     async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
    ///         if req.get_type() == "echo" {
    ///             let echo = req.body.clone().with_type("echo_ok");
    ///             return runtime.reply(req, echo).await;
    ///         }
    ///
    ///         // all other types are unsupported
    ///         done(runtime, req)
    ///     }
    /// }
    /// ```
    async fn process(self: &Self, runtime: Runtime, request: Message) -> Result<()>;
}

/// Returns a result with NotSupported error meaning that Node.process()
/// is not aware of specific message type or Ok(()) for init.
///
/// Example:
///
/// ```
/// use async_trait::async_trait;
/// use maelstrom::{Node, Runtime, Result, done};
/// use maelstrom::protocol::Message;
///
/// struct Handler {}
///
/// #[async_trait]
/// impl Node for Handler {
///     async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
///         // would skip init and respond with Code == 10 for any other type.
///         done(runtime, req)
///     }
/// }
/// ```
pub fn done(runtime: Runtime, message: Message) -> Result<()> {
    if message.get_type() == "init" {
        return Ok(());
    }

    let err = Error::NotSupported(message.body.typ.clone());
    let msg: ErrorMessageBody = err.clone().into();

    let runtime0 = runtime.clone();
    runtime.spawn(async move {
        let _ = runtime0.reply(message, msg).await;
    });

    return Err(Box::new(err));
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
                msg_id: AtomicU64::new(1),
                membership: OnceCell::new(),
                handler: OnceCell::new(),
                rpc: Mutex::default(),
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

        if !extra.contains_key("type") && !req.body.typ.is_empty() {
            extra.insert("type".to_string(), Value::String(req.body.typ + "_ok"));
        }

        let msg = Message {
            src: req.dest,
            dest: req.src,
            body: MessageBody::from_extra(extra).with_reply_to(req.body.msg_id),
        };

        let answer = serde_json::to_string(&msg)?;
        return self.send_raw(answer.as_str()).await;
    }

    pub async fn reply_ok(self: &Self, req: Message) -> Result<()> {
        return self.reply(req, Runtime::empty_response()).await;
    }

    pub async fn rpc<T>(runtime: Runtime, to: String, resp: T) -> Result<RPCResult>
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

        let req_msg_id = runtime.next_msg_id();
        let req = Message {
            src: runtime.node_id().to_string(),
            dest: to,
            body: MessageBody::from_extra(extra).and_msg_id(req_msg_id),
        };

        let (tx, rx) = oneshot::channel::<Message>();
        let _ = runtime.inter.rpc.lock().await.insert(req_msg_id, tx);

        let req_str = serde_json::to_string(&req)?;
        if let Err(err) = runtime.send_raw(req_str.as_str()).await {
            let _ = runtime.release_rpc_sender(req_msg_id).await;
            return Err(err);
        }

        return Ok(RPCResult::new(req_msg_id, rx, runtime.clone()));
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

        // new node = new message sequence
        self.inter.msg_id.store(1, Release);

        debug!("new {:?}", self.inter.membership.get().unwrap());

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

                            let tx_err0 = tx_err.clone();
                            self.spawn(Self::process_request(self.clone(), line).then(async move |result| {
                                if let Err(e) = result {
                                    if let Some(Error::NotSupported(t)) = e.downcast_ref::<Error>() {
                                        warn!("message type not supported: {}", t);
                                    } else {
                                        error!("process_request error: {}", e);
                                        let _ = tx_err0.send(Err(e)).await;
                                    }
                                }
                            }));
                        }
                        None => break
                    }
                },
                Some(e) = rx_err.recv() => { tx_out = e; break },
                else => break
            }
        }

        select! {
            _ = self.done() => {},
            Some(e) = rx_err.recv() => tx_out = e,
        }

        if tx_out.is_ok() {
            if let Ok(err) = rx_err.try_recv() {
                tx_out = err;
            }
        }

        rx_err.close();

        if let Err(e) = tx_out {
            debug!("node error: {}", e);
            return Err(e);
        }

        // TODO: print stats?
        debug!("node done");

        Ok(())
    }

    async fn process_request(runtime: Runtime, line: String) -> Result<()> {
        let msg = match serde_json::from_str::<Message>(line.as_str()) {
            Ok(v) => v,
            Err(err) => return Err(Box::new(err)),
        };

        // rpc call
        if msg.body.in_reply_to > 0 {
            let mut guard = runtime.inter.rpc.lock().await;
            if let Some(tx) = guard.remove(&msg.body.in_reply_to) {
                // we don't need to hold mutex for doing long blocking tx.send.
                drop(guard);
                // at the moment we don't care of the send err because I expect
                // it will fail only if the rx end is closed.
                drop(tx.send(msg));
            }
            return Ok(());
        }

        let is_init = msg.get_type() == "init";
        let mut init_source: Option<Message> = None;
        if is_init {
            init_source = Some(msg.clone());
            let res = runtime.process_init(&msg);
            if res.is_err() {
                return res;
            }
        }

        if let Some(handler) = runtime.inter.handler.get() {
            let res = handler.process(runtime.clone(), msg).await;
            if res.is_err() {
                return res;
            }
        }

        if is_init {
            let init_source_msg = init_source.unwrap();
            let init_resp: Value = serde_json::from_str(
                format!(
                    r#"{{"in_reply_to":{},"type":"init_ok"}}"#,
                    init_source_msg.body.msg_id
                )
                .as_str(),
            )?;
            return runtime.send(init_source_msg, init_resp).await;
        }

        Ok(())
    }

    fn process_init(self: &Self, message: &Message) -> Result<()> {
        let raw = message.body.extra.clone();
        let init = serde_json::from_value::<InitMessageBody>(Value::Object(raw))?;
        self.set_membership_state(MembershipState {
            node_id: init.node_id,
            nodes: init.nodes,
        })
    }

    #[inline]
    pub fn next_msg_id(self: &Self) -> u64 {
        return self.inter.msg_id.fetch_add(1, AcqRel);
    }

    #[inline]
    pub fn empty_response() -> Value {
        return Value::Object(serde_json::Map::default());
    }

    #[inline]
    pub(crate) async fn release_rpc_sender(&self, id: u64) -> Option<Sender<Message>> {
        return self.inter.rpc.lock().await.remove(&id);
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

// TODO: make err customizable
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
    use tokio::io::BufReader;
    use tokio_util::sync::CancellationToken;

    #[test]
    fn membership() -> Result<()> {
        let tokio_runtime = tokio::runtime::Runtime::new()?;
        tokio_runtime.block_on(async move {
            let runtime = Runtime::new();
            let runtime0 = runtime.clone();
            let s1 = MembershipState::example("n0", &["n0", "n1"]);
            let s2 = MembershipState::example("n1", &["n0", "n1"]);
            runtime.spawn(async move {
                runtime0.set_membership_state(s1).unwrap();
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
