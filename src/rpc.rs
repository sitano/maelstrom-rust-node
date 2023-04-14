use crate::protocol::Message;
use crate::Error;
use crate::Result;
use crate::Runtime;
use std::future::Future;
use std::pin::{pin, Pin};
use std::task::Poll;
use tokio::select;
use tokio::sync::oneshot::Receiver;
use tokio::sync::OnceCell;
use tokio_context::context::Context;

/// Represents a result of a RPC call. Can be awaited with or without timeout.
///
/// Example:
///
/// ```
/// use maelstrom::protocol::Message;
/// use maelstrom::{RPCResult, Runtime, Result};
/// use serde::Serialize;
/// use tokio_context::context::Context;
///
/// async fn call<T>(ctx: Context, runtime: Runtime, node: String, msg: T) -> Result<Message>
/// where
///     T: Serialize,
/// {
///     let mut res: RPCResult = Runtime::rpc(runtime, node, msg).await?;
///     return res.done_with(ctx).await;
/// }
/// ```
pub struct RPCResult {
    runtime: Runtime,
    rx: OnceCell<Receiver<Message>>,
    msg_id: u64,
}

impl RPCResult {
    pub fn new(msg_id: u64, rx: Receiver<Message>, runtime: Runtime) -> RPCResult {
        RPCResult {
            runtime,
            rx: OnceCell::new_with(Some(rx)),
            msg_id,
        }
    }

    /// Releases RPC call resources. Drop calls Self::done().
    ///
    /// Example:
    ///
    /// ```
    /// use maelstrom::protocol::Message;
    /// use maelstrom::{RPCResult, Runtime, Result};
    /// use serde::Serialize;
    ///
    /// async fn call<T>(runtime: Runtime, node: String, msg: T)
    /// where
    ///     T: Serialize,
    /// {
    ///     let _ = Runtime::rpc(runtime, node, msg).await;
    /// }
    /// ```
    pub fn done(&mut self) {
        drop(self.rx.take());
        drop(self.runtime.release_rpc_sender(self.msg_id));
    }

    /// Acquires a RPC call response within specific timeout.
    ///
    /// Example:
    ///
    /// ```
    /// use maelstrom::protocol::Message;
    /// use maelstrom::{RPCResult, Runtime, Result};
    /// use serde::Serialize;
    /// use tokio_context::context::Context;
    ///
    /// async fn call<T>(ctx: Context, runtime: Runtime, node: String, msg: T) -> Result<Message>
    /// where
    ///     T: Serialize,
    /// {
    ///     let mut res: RPCResult = Runtime::rpc(runtime, node, msg).await?;
    ///     return res.done_with(ctx).await;
    /// }
    /// ```
    pub async fn done_with(&mut self, mut ctx: Context) -> Result<Message> {
        let result: Result<Message>;
        let rx = match self.rx.take() {
            Some(x) => x,
            None => return Err(Box::new(Error::Abort)),
        };

        select! {
            data = rx => match data {
                Ok(resp) => result = rpc_msg_type(resp),
                Err(err) => result = Err(Box::new(err)),
            },
            _ = ctx.done() => result = Err(Box::new(Error::Timeout)),
        }

        drop(self.runtime.release_rpc_sender(self.msg_id));

        result
    }
}

impl Drop for RPCResult {
    fn drop(&mut self) {
        self.done();
    }
}

/// Makes RPCResult an awaitable future.
///
/// Example:
///
/// ```
/// use maelstrom::protocol::Message;
/// use maelstrom::{RPCResult, Runtime, Result};
/// use serde::Serialize;
///
/// async fn call<T>(runtime: Runtime, node: String, msg: T) -> Result<Message>
/// where
///     T: Serialize,
/// {
///     let mut res: RPCResult = Runtime::rpc(runtime, node, msg).await?;
///     return res.await;
/// }
/// ```
impl Future for RPCResult {
    type Output = Result<Message>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let rx = pin!(match self.rx.get_mut() {
            Some(x) => x,
            None => return Poll::Ready(Err(Box::new(Error::Abort))),
        });

        match rx.poll(cx) {
            Poll::Ready(t) => {
                let _ = self.rx.take();
                match t {
                    Err(e) => Poll::Ready(Err(Box::new(e))),
                    Ok(m) => Poll::Ready(rpc_msg_type(m)),
                }
            }
            _ => Poll::Pending,
        }
    }
}

fn rpc_msg_type(m: Message) -> Result<Message> {
    if m.body.is_error() {
        Err(Box::new(Error::from(&m.body)))
    } else {
        Ok(m)
    }
}
