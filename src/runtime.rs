#![allow(dead_code)]

use crate::WaitGroup;
use futures::FutureExt;
use log::info;
use simple_error::bail;
use std::future::Future;
use tokio::io::{AsyncWriteExt, Stdout};
use tokio::sync::{Mutex, OnceCell};
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Runtime {
    /// OnceCell seems works better here than RwLock, but what do we think of cluster membership change?
    /// How the API should behave if the Maelstrom will send the second init message?
    /// Let's pick the approach when it is possible to only once initialize a node and cluster
    /// membership change must start a new node and stop the old ones.
    membership: OnceCell<MembershipState>,

    // pub handlers: Arc<HashMap<String, HandlerFunc>>,

    // Output
    pub out: Mutex<Stdout>,

    serving: WaitGroup,
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct MembershipState {
    pub node_id: String,
    pub nodes: Vec<String>,
}

impl Runtime {
    pub fn new(out: Stdout) -> Self {
        return Runtime {
            membership: OnceCell::new(),
            out: Mutex::new(out),
            serving: WaitGroup::new(),
        };
    }

    pub async fn send_raw(self: &Self, msg: &str) -> Result<()> {
        {
            let mut out = self.out.lock().await;
            out.write_all(msg.as_bytes()).await?;
            out.write_all(b"\n").await?;
        }
        info!("Sent {}", msg);
        Ok(())
    }

    #[track_caller]
    pub fn spawn<T>(self: &Self, future: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let h = self.serving.clone();
        tokio::spawn(future.then(async move |x| {
            drop(h);
            x
        }))
    }

    pub fn node_id(self: &Self) -> &str {
        if let Some(v) = self.membership.get() {
            return v.node_id.as_str();
        }
        return &"";
    }

    pub fn nodes(self: &Self) -> &[String] {
        if let Some(v) = self.membership.get() {
            return v.nodes.as_slice();
        }
        return &[];
    }

    pub fn set_membership_state(self: &Self, state: MembershipState) -> Result<()> {
        if let Err(e) = self.membership.set(state) {
            bail!("membership is inited: {}", e);
        }
        Ok(())
    }

    pub async fn done(self: &Self) {
        self.serving.wait().await;
    }

    // TODO: need also sync done
}

#[cfg(test)]
mod test {
    use crate::runtime::{MembershipState, Result};
    use crate::Runtime;
    use log::debug;
    use std::sync::Arc;
    use tokio::io::stdout;

    #[test]
    fn membership() -> Result<()> {
        let tokio_runtime = tokio::runtime::Runtime::new()?;
        tokio_runtime.block_on(async move {
            // TODO: use fake stdout
            let runtime = Arc::new(Runtime::new(stdout()));
            let runtime0 = runtime.clone();
            runtime.spawn(async move {
                runtime0
                    .set_membership_state(MembershipState::example("n0", &["n0", "n1"]))
                    .unwrap();
                debug!("{}", runtime0.node_id());
                async move {
                    assert!(matches!(
                        runtime0
                            .set_membership_state(MembershipState::example("n1", &["n0", "n1"])),
                        Err(_)
                    ));
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
