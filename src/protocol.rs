#![allow(dead_code)]

use serde::Deserialize;
use serde::Serialize;
use serde_json::{Map, Value};

/// Message represents a message sent from Src node to Dest node.
/// The body is stored as parsed MessageBody along with the original string
/// and all extra fields.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Default)]
pub struct Message {
    pub src: String,
    pub dest: String,
    pub body: MessageBody,
}

/// MessageBody represents the reserved keys for a message body.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Default)]
pub struct MessageBody {
    /// Message type.
    #[serde(rename = "type", default, skip_serializing_if = "String::is_empty")]
    pub typo: String,

    /// Optional. Message identifier that is unique to the source node.
    #[serde(default, skip_serializing_if = "i32_zero_by_ref")]
    pub msg_id: i32,

    /// Optional. For request/response, the msg_id of the request.
    #[serde(default, skip_serializing_if = "i32_zero_by_ref")]
    pub in_reply_to: i32,

    /// Error code, if an error occurred.
    #[serde(default, skip_serializing_if = "i32_zero_by_ref")]
    pub code: i32,

    /// Error message, if an error occurred.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,

    /// All the fields not mentioned here
    #[serde(flatten)]
    pub extra: Map<String, Value>,

    /// Original body message
    #[serde(skip)]
    pub raw: String,
}

/// This is only used for serialize
fn i32_zero_by_ref(num: &i32) -> bool {
    *num == 0
}

/// InitMessageBody represents the message body for the "init" message.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Default)]
pub struct InitMessageBody {
    /// Node id.
    #[serde(default)]
    pub node_id: String,

    /// Neighbours.
    #[serde(rename = "node_ids", default)]
    pub nodes: Vec<String>,
}

impl Message {
    /// RPCError returns the RPC error from the message body.
    /// Returns a malformed body as a generic crash error.
    fn rpc_error<T>(self: &Self) -> T {
        self.body.rpc_error()
    }
}

impl MessageBody {
    pub fn new() -> Self {
        Self::default()
    }

    /// RPCError returns the RPC error from the message body.
    /// Returns a malformed body as a generic crash error.
    pub fn rpc_error<T>(self: &Self) -> T {
        panic!("TODO")
    }

    pub fn from_error() -> Self {
        panic!("TODO")
    }

    pub fn with_type(self, typ: &str) -> Self {
        let mut t = self;
        t.typo = typ.to_string();
        return t;
    }

    pub fn with_str_error(self, code: i32, err: &str) -> Self {
        let mut t = self;
        t.code = code;
        t.text = err.to_string();
        return t;
    }

    pub fn with_reply_to(self, in_reply_to: i32) -> Self {
        let mut t = self;
        t.in_reply_to = in_reply_to;
        return t;
    }

    pub fn from_str_error(code: i32, err: &str) -> Self {
        Self::new().with_type("error").with_str_error(code, err)
    }

    pub fn from_extra(extra: Map<String, Value>) -> Self {
        let mut t = Self::default();
        t.extra = extra;
        return t;
    }
}

#[cfg(test)]
mod test {
    use crate::protocol::{InitMessageBody, Message, MessageBody};
    use crate::runtime::Result;
    use serde_json::{Map, Value};

    #[test]
    fn parse_message() -> Result<()> {
        let echo = "{ \"src\": \"c1\", \"dest\": \"n1\", \"body\": { \"type\": \"echo\", \"msg_id\": 1, \"echo\": \"Please echo 35\" }}";
        let mut msg: Message = serde_json::from_str(&echo)?;
        msg.body.raw = serde_json::to_string(&msg.body)?;
        assert_eq!(
            msg,
            Message {
                src: "c1".to_string(),
                dest: "n1".to_string(),
                body: MessageBody {
                    typo: "echo".to_string(),
                    msg_id: 1,
                    in_reply_to: 0,
                    code: 0,
                    text: "".to_string(),
                    extra: Map::from_iter([(
                        "echo".to_string(),
                        Value::String("Please echo 35".to_string())
                    )]),
                    raw: "{\"type\":\"echo\",\"msg_id\":1,\"echo\":\"Please echo 35\"}".to_string()
                },
            }
        );
        Ok(())
    }

    #[test]
    fn parse_init_message() -> Result<()> {
        let init =
            "{\"type\":\"init\",\"msg_id\":1,\"node_id\":\"n0\",\"node_ids\":[\"n0\",\"n1\"]}";
        let msg: InitMessageBody = serde_json::from_str(&init)?;
        assert_eq!(
            msg,
            InitMessageBody {
                node_id: "n0".to_string(),
                nodes: Vec::from(["n0".to_string(), "n1".to_string()]),
            }
        );
        Ok(())
    }
}