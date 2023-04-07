#![allow(dead_code)]

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

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
    pub extra: HashMap<String, Value>,

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
    /// RPCError returns the RPC error from the message body.
    /// Returns a malformed body as a generic crash error.
    fn rpc_error<T>(self: &Self) -> T {
        panic!("TODO")
    }
}

// HandlerFunc is the function signature for a message handler.
// type HandlerFunc = dyn FnOnce(Runtime, Message) -> dyn Future<Output = Error>;

#[cfg(test)]
mod test {
    use crate::protocol::{InitMessageBody, Message, MessageBody};
    use serde_json::Value;
    use std::collections::HashMap;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

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
                    extra: HashMap::from([(
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
