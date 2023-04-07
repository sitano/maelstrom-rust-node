#![allow(dead_code)]

use crate::WaitGroup;
use futures::FutureExt;
use log::info;
use std::future::Future;
use tokio::io::{AsyncWriteExt, Stdout};
use tokio::sync::{Mutex, OnceCell};
use tokio::task::JoinHandle;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub struct Runtime {
    pub membership: OnceCell<MembershipState>,

    // pub handlers: Arc<HashMap<String, HandlerFunc>>,

    // Output
    pub out: Mutex<Stdout>,

    // TODO: make private + drop
    pub serving: WaitGroup,
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
        if let Some(state) = self.membership.get() {
            return state.node_id.as_str();
        }
        return "";
    }

    pub fn nodes(self: &Self) -> &[String] {
        if let Some(state) = self.membership.get() {
            return state.nodes.as_slice();
        }
        return &[];
    }

    // not sure if mut requirement will work out for us in the future
    pub fn set_membership_state(self: &mut Self, state: MembershipState) {
        self.membership.take();
        self.membership.set(state).unwrap();
    }
}

#[cfg(test)]
mod test {
    use crate::runtime::{MembershipState, Result};
    use crate::Runtime;
    use tokio::io::stdout;

    #[test]
    fn membership() -> Result<()> {
        let mut runtime = Runtime::new(stdout());
        let state1 = MembershipState {
            node_id: "n0".to_string(),
            nodes: Vec::from(["n0".to_string(), "n1".to_string()]),
        };
        let state2 = MembershipState {
            node_id: "n1".to_string(),
            nodes: Vec::from(["n0".to_string(), "n1".to_string()]),
        };

        runtime.set_membership_state(state1);
        assert_eq!(runtime.node_id(), "n0", "invalid node id");

        runtime.set_membership_state(state2);
        assert_eq!(runtime.node_id(), "n1", "invalid node id");

        Ok(())
    }
}
