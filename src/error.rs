use crate::protocol::ErrorMessageBody;
use std::fmt::{Display, Formatter};

/// [source](https://github.com/jepsen-io/maelstrom/blob/main/doc/protocol.md#errors).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Error {
    Timeout,
    NotSupported,
    TemporarilyUnavailable,
    MalformedRequest,
    Crash,
    Abort,
    KeyDoesNotExist,
    KeyAlreadyExists,
    PreconditionFailed,
    TxnConflict,
    Custom(i32, String),
}

impl Error {
    pub fn code(&self) -> i32 {
        match self {
            Error::Timeout => 0,
            Error::NotSupported => 10,
            Error::TemporarilyUnavailable => 11,
            Error::MalformedRequest => 12,
            Error::Crash => 13,
            Error::Abort => 14,
            Error::KeyDoesNotExist => 20,
            Error::KeyAlreadyExists => 21,
            Error::PreconditionFailed => 22,
            Error::TxnConflict => 30,
            Error::Custom(code, _) => *code,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Error::Timeout => "timeout",
            Error::NotSupported => "not supported",
            Error::TemporarilyUnavailable => "temporarily unavailable",
            Error::MalformedRequest => "malformed request",
            Error::Crash => "crash",
            Error::Abort => "abort",
            Error::KeyDoesNotExist => "key does not exist",
            Error::KeyAlreadyExists => "key already exists",
            Error::PreconditionFailed => "precondition failed",
            Error::TxnConflict => "txn conflict",
            Error::Custom(_, text) => text.as_str(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "error({}): {}", self.code(), self.description())
    }
}

impl std::error::Error for Error {}

impl Into<ErrorMessageBody> for Error {
    fn into(self) -> ErrorMessageBody {
        return ErrorMessageBody {
            typ: "error".to_string(),
            code: self.code(),
            text: self.description().to_string(),
        };
    }
}
