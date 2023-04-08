#![allow(dead_code)]

use crate::WaitGroup;
use futures::FutureExt;
use log::info;
use std::future::Future;
use std::sync::{LockResult, RwLock, RwLockReadGuard};
use tokio::io::{AsyncWriteExt, Stdout};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Runtime {
    membership: RwLock<MembershipState>,

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
            membership: RwLock::new(MembershipState::default()),
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

    pub fn membership(self: &Self) -> LockResult<RwLockReadGuard<'_, MembershipState>> {
        return self.membership.read();
    }

    pub fn set_membership_state(self: &Self, state: MembershipState) {
        *self.membership.write().unwrap() = state;
    }

    pub async fn done(self: &Self) {
        self.serving.wait().await;
    }
}

#[cfg(test)]
mod test {
    use std::sync::Arc;
    use log::debug;
    use crate::runtime::{MembershipState, Result};
    use crate::Runtime;
    use tokio::io::stdout;

    #[test]
    fn membership() -> Result<()> {
        let tokio_runtime = tokio::runtime::Runtime::new()?;
        tokio_runtime.block_on(async move {
            let runtime = Arc::new(Runtime::new(stdout()));
            let runtime0 = runtime.clone();
            runtime.spawn(async move {
                runtime0.set_membership_state(MembershipState::example("n0", &["n0", "n1"]));
                debug!("{}", runtime0.membership().unwrap().node_id);
                async move {
                    runtime0.set_membership_state(MembershipState::example("n1", &["n0", "n1"]));
                }.await;
            });
            runtime.done().await;
            assert_eq!(
                runtime.membership().unwrap().node_id,
                "n1",
                "invalid node id"
            );
        });
        Ok(())
    }

    impl MembershipState {
        fn example(n: &str, s: &[&str]) -> Self {
            return MembershipState{
                node_id: n.to_string(),
                nodes: s.iter().map(|x| x.to_string()).collect(),
            }
        }
    }
}
