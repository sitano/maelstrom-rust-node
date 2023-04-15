#![allow(dead_code)]

use crate::error::Error;
use crate::protocol::{ErrorMessageBody, InitMessageBody, Message};
use crate::{rpc_err_to_response, RPCResult};
use crate::waitgroup::WaitGroup;
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
use tokio::sync::{mpsc, Mutex, OnceCell, oneshot};
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
    async fn process(&self, runtime: Runtime, request: Message) -> Result<()>;
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

    Err(Box::new(err))
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
        Runtime::default()
    }

    pub fn with_handler(self, handler: Arc<dyn Node + Send + Sync>) -> Self {
        if self.inter.handler.set(handler).is_err() {
            panic!("runtime handler is already initialized");
        }
        self
    }

    pub async fn send_raw(&self, msg: &str) -> Result<()> {
        {
            let mut out = self.inter.out.lock().await;
            out.write_all(msg.as_bytes()).await?;
            out.write_all(b"\n").await?;
        }
        info!("Sent {}", msg);
        Ok(())
    }

    pub fn send_async<T>(&self, to: String, message: T) -> Result<()>
    where
        T: Serialize + Send,
    {
        let runtime = self.clone();
        let msg = crate::protocol::message(self.node_id().to_string(), to, message)?;
        let ans = serde_json::to_string(&msg)?;
        self.spawn(async move {
            if let Err(err) = runtime.send_raw(ans.as_str()).await {
                error!("send error: {}", err);
            }
        });
        Ok(())
    }

    pub async fn send<T>(&self, to: String, message: T) -> Result<()>
    where
        T: Serialize,
    {
        let msg = crate::protocol::message(self.node_id().to_string(), to, message)?;
        let ans = serde_json::to_string(&msg)?;
        self.send_raw(ans.as_str()).await
    }

    pub async fn send_back<T>(&self, req: Message, resp: T) -> Result<()>
    where
        T: Serialize,
    {
        self.send(req.src, resp).await
    }

    pub async fn reply<T>(&self, req: Message, resp: T) -> Result<()>
    where
        T: Serialize,
    {
        let mut msg = crate::protocol::message(self.node_id().to_string(), req.src, resp)?;
        msg.body.in_reply_to = req.body.msg_id;

        if !msg.body.extra.contains_key("type") && !req.body.typ.is_empty() {
            let key = "type".to_string();
            let value = Value::String(req.body.typ + "_ok");
            msg.body.extra.insert(key, value);
        }

        let answer = serde_json::to_string(&msg)?;
        self.send_raw(answer.as_str()).await
    }

    pub async fn reply_ok(&self, req: Message) -> Result<()> {
        self.reply(req, Runtime::empty_response()).await
    }

    #[track_caller]
    pub fn spawn<T>(&self, future: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let h = self.inter.serving.clone();
        tokio::spawn(future.then(|x| async move {
            drop(h);
            x
        }))
    }

    /// Example:
    /// ```
    /// use maelstrom::{Error, Result, Runtime};
    /// use std::fmt::{Display, Formatter};
    /// use serde::Serialize;
    /// use serde::Deserialize;
    /// use tokio_context::context::Context;
    ///
    /// pub struct Storage {
    ///     typ: &'static str,
    ///     runtime: Runtime,
    /// }
    ///
    /// impl Storage {
    ///     async fn get<T>(&self, ctx: Context, key: String) -> Result<T>
    ///         where
    ///             T: Deserialize<'static> + Send,
    ///     {
    ///         let req = Message::Read::<String> { key };
    ///         let mut call = self.runtime.rpc(self.typ.to_string(), req).await?;
    ///         let msg = call.done_with(ctx).await?;
    ///         let data = msg.body.as_obj::<Message<T>>()?;
    ///         match data {
    ///             Message::ReadOk { value } => Ok(value),
    ///             _ => Err(Box::new(Error::Custom(
    ///                 -1,
    ///                 "kv: protocol violated".to_string(),
    ///             ))),
    ///         }
    ///     }
    /// }
    ///
    /// #[derive(Serialize, Deserialize)]
    /// #[serde(rename_all = "snake_case", tag = "type")]
    /// enum Message<T> {
    ///     Read {
    ///         key: String,
    ///     },
    ///     ReadOk {
    ///         value: T,
    ///     },
    /// }
    /// ```
    pub fn rpc<T>(&self, to: String, request: T) -> impl Future<Output = Result<RPCResult>>
        where
            T: Serialize,
    {
        let runtime = self.clone();

        async move {
            let mut msg = crate::protocol::message(runtime.node_id().to_string(), to, request)?;
            let req_msg_id = runtime.next_msg_id();

            msg.body.msg_id = req_msg_id;

            let (tx, rx) = oneshot::channel::<Message>();
            let _ = runtime.insert_rpc_sender(req_msg_id, tx).await;

            let req_str = serde_json::to_string(&msg)?;
            if let Err(err) = runtime.send_raw(req_str.as_str()).await {
                let _ = runtime.release_rpc_sender(req_msg_id).await;
                return Err(err);
            }

            Ok(RPCResult::new(req_msg_id, rx, runtime))
        }
    }

    pub fn node_id(&self) -> &str {
        if let Some(v) = self.inter.membership.get() {
            return v.node_id.as_str();
        }
        ""
    }

    pub fn nodes(&self) -> &[String] {
        if let Some(v) = self.inter.membership.get() {
            return v.nodes.as_slice();
        }
        &[]
    }

    pub fn set_membership_state(&self, state: MembershipState) -> Result<()> {
        if let Err(e) = self.inter.membership.set(state) {
            bail!("membership is inited: {}", e);
        }

        // new node = new message sequence
        self.inter.msg_id.store(1, Release);

        debug!("new {:?}", self.inter.membership.get().unwrap());

        Ok(())
    }

    pub async fn done(&self) {
        self.inter.serving.wait().await;
    }

    pub async fn run(&self) -> Result<()> {
        self.run_with(BufReader::new(stdin())).await
    }

    pub async fn run_with<R>(&self, input: BufReader<R>) -> Result<()>
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
                            self.spawn(Self::process_request(self.clone(), line).then(|result| async move  {
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

        let mut init_source: Option<(String, u64)> = None;
        let is_init = msg.get_type() == "init";
        if is_init {
            init_source = Some((msg.src.clone(), msg.body.msg_id));
            runtime.process_init(&msg)?;
        }

        if let Some(handler) = runtime.inter.handler.get() {
            // I am not happy we are cloning a msg here, but let it go this time.
            let res = handler.process(runtime.clone(), msg.clone()).await;
            if res.is_err() {
                // rpc error is user level error
                if let Some(user_err) = rpc_err_to_response(&res) {
                    runtime.reply(msg, user_err).await?;
                } else {
                    return res;
                }
            }
        }

        if is_init {
            let (dst, msg_id) = init_source.unwrap();
            let init_resp: Value = serde_json::from_str(
                format!(r#"{{"in_reply_to":{},"type":"init_ok"}}"#, msg_id).as_str(),
            )?;
            return runtime.send(dst, init_resp).await;
        }

        Ok(())
    }

    fn process_init(&self, message: &Message) -> Result<()> {
        let raw = message.body.extra.clone();
        let init = serde_json::from_value::<InitMessageBody>(Value::Object(raw))?;
        self.set_membership_state(MembershipState {
            node_id: init.node_id,
            nodes: init.nodes,
        })
    }

    #[inline]
    pub fn next_msg_id(&self) -> u64 {
        self.inter.msg_id.fetch_add(1, AcqRel)
    }

    #[inline]
    pub fn empty_response() -> Value {
        Value::Object(serde_json::Map::default())
    }

    #[inline]
    pub(crate) async fn insert_rpc_sender(
        &self,
        id: u64,
        tx: Sender<Message>,
    ) -> Option<Sender<Message>> {
        self.inter.rpc.lock().await.insert(id, tx)
    }

    #[inline]
    pub(crate) async fn release_rpc_sender(&self, id: u64) -> Option<Sender<Message>> {
        self.inter.rpc.lock().await.remove(&id)
    }

    #[inline]
    pub fn is_client(&self, src: &String) -> bool {
        !src.is_empty() && src.starts_with('c')
    }

    #[inline]
    pub fn is_from_cluster(&self, src: &String) -> bool {
        // alternative implementation: self.nodes().contains(src)
        !src.is_empty() && src.starts_with('n')
    }

    /// All nodes that are not this node.
    #[inline]
    pub fn neighbours(&self) -> impl Iterator<Item = &String> {
        let n = self.node_id();
        self.nodes()
            .iter()
            .filter(move |t: &&String| t.as_str() != n)
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Runtime {
            inter: Arc::new(Inter {
                msg_id: AtomicU64::new(1),
                membership: OnceCell::new(),
                handler: OnceCell::new(),
                rpc: Mutex::default(),
                out: Mutex::new(stdout()),
                serving: WaitGroup::new(),
            }),
        }
    }
}

impl Clone for Runtime {
    fn clone(&self) -> Self {
        Runtime {
            inter: self.inter.clone(),
        }
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub struct BlackHoleNode {}

#[async_trait]
impl Node for BlackHoleNode {
    async fn process(&self, _: Runtime, _: Message) -> Result<()> {
        Ok(())
    }
}

// TODO: make err customizable
#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub struct IOFailingNode {}

#[async_trait]
impl Node for IOFailingNode {
    async fn process(&self, _: Runtime, _: Message) -> Result<()> {
        bail!("IOFailingNode: process failed")
    }
}

#[derive(Default, Copy, Clone, PartialEq, Eq, Debug)]
pub struct EchoNode {}

#[async_trait]
impl Node for EchoNode {
    async fn process(&self, runtime: Runtime, req: Message) -> Result<()> {
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
