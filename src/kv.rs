use crate::{Error, Result, Runtime};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use tokio_context::context::Context;

#[async_trait]
pub trait KV: Clone + Display + Send + Sync {
    /// Get returns the value for a given key in the key/value store.
    /// Returns an RPCError error with a KeyDoesNotExist code if the key does not exist.
    async fn get<T>(&self, ctx: Context, key: String) -> Result<T>
    where
        T: Deserialize<'static> + Send;

    /// Put overwrites the value for a given key in the key/value store.
    async fn put<T>(&self, ctx: Context, key: String, val: T) -> Result<()>
    where
        T: Serialize + Send;

    /// CAS updates the value for a key if its current value matches the
    /// previous value. Creates the key if it is not exist is requested.
    ///
    /// Returns an RPCError with a code of PreconditionFailed if the previous value
    /// does not match. Return a code of KeyDoesNotExist if the key did not exist.
    async fn cas<T>(&self, ctx: Context, key: String, from: T, to: T, put: bool) -> Result<()>
    where
        T: Serialize + Deserialize<'static> + Send;
}

#[derive(Clone)]
pub struct Storage {
    typ: &'static str,
    runtime: Runtime,
}

/// Creates a linearizable storage.
pub fn lin_kv(runtime: Runtime) -> Storage {
    Storage {
        typ: "lin-kv",
        runtime,
    }
}

/// Creates a sequentially consistent storage.
pub fn seq_kv(runtime: Runtime) -> Storage {
    Storage {
        typ: "seq-kv",
        runtime,
    }
}

/// Creates last-write-wins storage type.
pub fn lww_kv(runtime: Runtime) -> Storage {
    Storage {
        typ: "lww-kv",
        runtime,
    }
}

/// Creates total-store-order kind of storage.
pub fn tso_kv(runtime: Runtime) -> Storage {
    Storage {
        typ: "lin-tso",
        runtime,
    }
}

impl Display for Storage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Storage({})", self.typ)
    }
}

#[async_trait]
impl KV for Storage {
    async fn get<T>(&self, ctx: Context, key: String) -> Result<T>
    where
        T: Deserialize<'static> + Send,
    {
        let req = Message::Read::<String> { key };
        let mut call = self.runtime.rpc(self.typ.to_string(), req).await?;
        let msg = call.done_with(ctx).await?;
        let data = msg.body.as_obj::<Message<T>>()?;
        match data {
            Message::ReadOk { value } => Ok(value),
            _ => Err(Box::new(Error::Custom(
                -1,
                "kv: protocol violated".to_string(),
            ))),
        }
    }

    async fn put<T>(&self, ctx: Context, key: String, value: T) -> Result<()>
    where
        T: Serialize + Send,
    {
        let req = Message::Write::<T> { key, value };

        let mut call = self.runtime.rpc(self.typ.to_string(), req).await?;
        let _msg = call.done_with(ctx).await?;
        Ok(())
    }

    async fn cas<T>(&self, ctx: Context, key: String, from: T, to: T, put: bool) -> Result<()>
    where
        T: Serialize + Deserialize<'static> + Send,
    {
        let req = Message::Cas::<T> { key, from, to, put };
        let mut call = self.runtime.rpc( self.typ.to_string(), req).await?;
        let _msg = call.done_with(ctx).await?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum Message<T> {
    /// KVReadMessageBody represents the body for the KV "read" message.
    Read {
        key: String,
    },
    /// KVReadOKMessageBody represents the response body for the KV "read_ok" message.
    ReadOk {
        value: T,
    },
    /// KVWriteMessageBody represents the body for the KV "cas" message.
    Write {
        key: String,
        value: T,
    },
    /// KVCASMessageBody represents the body for the KV "cas" message.
    Cas {
        key: String,
        from: T,
        to: T,
        #[serde(
            default,
            rename = "create_if_not_exists",
            skip_serializing_if = "is_ref_false"
        )]
        put: bool,
    },
    CasOk {},
}

fn is_ref_false(b: &bool) -> bool {
    !*b
}
