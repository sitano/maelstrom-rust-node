use crate::protocol::ErrorMessageBody;
use std::fmt::{Display, Formatter};

/// [source](https://github.com/jepsen-io/maelstrom/blob/main/doc/protocol.md#errors).
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Error {
    /// Indicates that the requested operation could not be completed within a timeout.
    Timeout,
    /// Use this error to indicate that a requested operation is not supported by
    /// the current implementation. Helpful for stubbing out APIs during development.
    NotSupported(String),
    /// Indicates that the operation definitely cannot be performed at this time--perhaps
    /// because the server is in a read-only state, has not yet been initialized,
    /// believes its peers to be down, and so on. Do not use this error for indeterminate
    /// cases, when the operation may actually have taken place.
    TemporarilyUnavailable,
    /// The client's request did not conform to the server's expectations,
    /// and could not possibly have been processed.
    MalformedRequest,
    /// Indicates that some kind of general, indefinite error occurred.
    /// Use this as a catch-all for errors you can't otherwise categorize,
    /// or as a starting point for your error handler: it's safe to return
    /// internal-error for every problem by default, then add special cases
    /// for more specific errors later.
    Crash,
    /// Indicates that some kind of general, definite error occurred.
    /// Use this as a catch-all for errors you can't otherwise categorize,
    /// when you specifically know that the requested operation has not taken place.
    /// For instance, you might encounter an indefinite failure during
    /// the prepare phase of a transaction: since you haven't started the commit process yet,
    /// the transaction can't have taken place. It's therefore safe to return
    /// a definite abort to the client.
    Abort,
    /// The client requested an operation on a key which does not exist
    /// (assuming the operation should not automatically create missing keys).
    KeyDoesNotExist,
    /// The client requested the creation of a key which already exists,
    /// and the server will not overwrite it.
    KeyAlreadyExists,
    /// The requested operation expected some conditions to hold,
    /// and those conditions were not met. For instance, a compare-and-set operation
    /// might assert that the value of a key is currently 5; if the value is 3,
    /// the server would return precondition-failed.
    PreconditionFailed,
    /// The requested transaction has been aborted because of a conflict with
    /// another transaction. Servers need not return this error on every conflict:
    /// they may choose to retry automatically instead.
    TxnConflict,
    /// Custom error code for anything you would like to add.
    Custom(i32, String),
}

impl Error {
    pub fn code(&self) -> i32 {
        match self {
            Error::Timeout => 0,
            Error::NotSupported(_) => 10,
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
            Error::NotSupported(t) => t.as_str(),
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
