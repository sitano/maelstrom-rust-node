#![allow(dead_code)]

use crate::{Error, Result};
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
    pub typ: String,

    /// Optional. Message identifier that is unique to the source node.
    #[serde(default, skip_serializing_if = "u64_zero_by_ref")]
    pub msg_id: u64,

    /// Optional. For request/response, the msg_id of the request.
    #[serde(default, skip_serializing_if = "u64_zero_by_ref")]
    pub in_reply_to: u64,

    /// All the fields not mentioned here
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

fn u64_zero_by_ref(num: &u64) -> bool {
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

/// ErrorMessageBody represents the error response body.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq, Default)]
pub struct ErrorMessageBody {
    /// Message type.
    #[serde(rename = "type")]
    pub typ: String,

    /// Error code, if an error occurred.
    #[serde(default)]
    pub code: i32,

    /// Error message, if an error occurred.
    #[serde(default)]
    pub text: String,
}

impl Message {
    pub fn get_type(&self) -> &str {
        return self.body.typ.as_str();
    }
}

impl MessageBody {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_type(self, typ: &str) -> Self {
        let mut t = self;
        t.typ = typ.to_string();
        t
    }

    pub fn with_reply_to(self, in_reply_to: u64) -> Self {
        let mut t = self;
        t.in_reply_to = in_reply_to;
        t
    }

    pub fn and_msg_id(self, msg_id: u64) -> Self {
        let mut t = self;
        t.msg_id = msg_id;
        t
    }

    pub fn from_extra(extra: Map<String, Value>) -> Self {
        MessageBody {
            extra,
            ..Default::default()
        }
    }

    pub fn is_error(&self) -> bool {
        self.typ == "error"
    }

    /// ```
    /// use maelstrom::protocol::Message;
    /// use serde_json::Error;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct BroadcastRequest {}
    ///
    /// fn parse(m: Message) -> Result<BroadcastRequest, Error> {
    ///     serde_json::from_value::<BroadcastRequest>(m.body.raw())
    /// }
    /// ```
    pub fn raw(&self) -> Value {
        // we could name it to reflect cloning, but ok.
        // we also could re-serialize whole self to Value first, but probably not needed.
        // users usually need at least type to serialize it into the errors.
        let mut raw = self.extra.clone();
        drop(raw.insert("type".to_string(), Value::String(self.typ.clone())));
        Value::Object(raw)
    }

    /// ```
    /// use maelstrom::Result;
    /// use maelstrom::protocol::Message;
    /// use serde_json::Error;
    ///
    /// #[derive(serde::Deserialize)]
    /// struct BroadcastRequest {}
    ///
    /// fn parse(m: Message) -> Result<BroadcastRequest> {
    ///     m.body.as_obj::<BroadcastRequest>()
    /// }
    /// ```
    pub fn as_obj<'de, T>(&self) -> Result<T>
    where
        T: Deserialize<'de>,
    {
        match T::deserialize(self.raw()) {
            Ok(t) => Ok(t),
            Err(e) => Err(Box::new(e)),
        }
    }
}

impl ErrorMessageBody {
    pub fn new(code: i32, text: &str) -> Self {
        ErrorMessageBody {
            typ: "error".to_string(),
            code,
            text: text.to_string(),
        }
    }

    pub fn from_error(err: Error) -> Self {
        Self::from(err)
    }
}

impl From<Error> for ErrorMessageBody {
    fn from(value: Error) -> Self {
        return ErrorMessageBody {
            typ: "error".to_string(),
            code: value.code(),
            text: match value {
                Error::NotSupported(t) => format!("{} message type is not supported", t),
                Error::Custom(id, t) => format!("error({}): {}", id, t),
                o => o.description().to_string(),
            },
        };
    }
}

#[cfg(test)]
mod test {
    use crate::protocol::{InitMessageBody, Message, MessageBody};
    use crate::runtime::Result;
    use serde_json::{Map, Value};

    #[test]
    fn parse_message() -> Result<()> {
        let echo = r#"{ "src": "c1", "dest": "n1", "body": { "type": "echo", "msg_id": 1, "echo": "Please echo 35" }}"#;

        let msg = serde_json::from_str::<Message>(&echo)?;
        let expected = Message {
            src: "c1".to_string(),
            dest: "n1".to_string(),
            body: MessageBody::from_extra(Map::from_iter([(
                "echo".to_string(),
                Value::String("Please echo 35".to_string()),
            )]))
            .with_type("echo")
            .and_msg_id(1),
        };
        assert_eq!(msg, expected);
        Ok(())
    }

    #[test]
    fn parse_init_message() -> Result<()> {
        let init = r#"{"type":"init","msg_id":1,"node_id":"n0","node_ids":["n0","n1"]}"#;
        let msg: InitMessageBody = serde_json::from_str(&init)?;
        let expect = InitMessageBody::example("n0", &["n0", "n1"]);
        assert_eq!(msg, expect);
        Ok(())
    }

    impl InitMessageBody {
        fn example(n: &str, s: &[&str]) -> Self {
            return InitMessageBody {
                node_id: n.to_string(),
                nodes: s.iter().map(|x| x.to_string()).collect(),
            };
        }
    }
}
