use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

/// Message represents a message sent from Src node to Dest node.
/// The body is stored as unparsed JSON so the handler can parse it itself.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct Message {
    pub src: String,
    pub dest: String,
    pub body: MessageBody,
}

/// MessageBody represents the reserved keys for a message body.
#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MessageBody {
    /// Message type.
    #[serde(rename = "type", default)]
    typo: String,

    /// Optional. Message identifier that is unique to the source node.
    #[serde(default)]
    msg_id: i32,

    /// Optional. For request/response, the msg_id of the request.
    #[serde(default)]
    in_reply_to: i32,

    /// Error code, if an error occurred.
    #[serde(default)]
    code: i32,

    /// Error message, if an error occurred.
    #[serde(default)]
    text: String,

    #[serde(flatten)]
    extra: HashMap<String, Value>,
}

#[cfg(test)]
mod test {
    use crate::protocol::{Message, MessageBody};
    use serde_json::Value;
    use std::collections::HashMap;

    type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

    #[test]
    fn parse_message() -> Result<()> {
        let echo = "{ \"src\": \"c1\", \"dest\": \"n1\", \"body\": { \"type\": \"echo\", \"msg_id\": 1, \"echo\": \"Please echo 35\" }}";
        let j: Message = serde_json::from_str(&echo)?;
        assert_eq!(
            j,
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
                    )])
                },
            }
        );
        Ok(())
    }
}
