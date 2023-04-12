use crate::protocol::Message;
use crate::Error;
use crate::Result;
use crate::Runtime;
use std::future::Future;
use std::pin::Pin;
use std::task::Poll;
use tokio::select;
use tokio::sync::oneshot::Receiver;
use tokio::sync::OnceCell;
use tokio_context::context::Context;

pub struct RPCResult {
    runtime: Runtime,
    rx: OnceCell<Receiver<Message>>,
    msg_id: u64,
}

impl RPCResult {
    pub fn new(msg_id: u64, rx: Receiver<Message>, runtime: Runtime) -> RPCResult {
        return RPCResult {
            runtime,
            rx: OnceCell::new_with(Some(rx)),
            msg_id,
        };
    }

    pub fn done(&self) {
        let _ = self.runtime.release_rpc_sender(self.msg_id);
    }

    pub async fn done_with(&mut self, mut ctx: Context) -> Result<Message> {
        let result: Result<Message>;
        let rx = match self.rx.take() {
            Some(x) => x,
            None => return Err(Box::new(Error::Abort)),
        };

        select! {
            data = rx => match data {
                Ok(resp) => result = Ok(resp),
                Err(err) => result = Err(Box::new(err)),
            },
            _ = ctx.done() => result = Err(Box::new(Error::Timeout)),
        }

        let _ = self.runtime.release_rpc_sender(self.msg_id);

        return result;
    }
}

impl Drop for RPCResult {
    fn drop(&mut self) {
        self.done();
    }
}

impl Future for RPCResult {
    type Output = Result<Message>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        let rx = match self.rx.get_mut() {
            Some(x) => x,
            None => return Poll::Ready(Err(Box::new(Error::Abort))),
        };

        let mut o = Box::pin(rx);
        match o.as_mut().poll(cx) {
            Poll::Ready(Ok(t)) => {
                let _ = self.rx.take();
                Poll::Ready(Ok(t))
            }
            Poll::Ready(Err(t)) => {
                let _ = self.rx.take();
                Poll::Ready(Err(Box::new(t)))
            }
            _ => Poll::Pending,
        }
    }
}
