use thiserror::Error;

#[derive(Error, Debug)]
pub enum IncluderError {
    #[error("Connection failed")]
    Connection,
    #[error("RPC call failed: {0}")]
    RPCError(String),
}

#[derive(Error, Debug)]
pub enum RefundManagerError {
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),
    #[error("Invalid recipient: {0}")]
    InvalidRecipient(String),
    #[error("Generic error: {0}")]
    GenericError(String),
}

#[derive(Error, Debug)]
pub enum BroadcasterError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("RPC Call Failed: {0}")]
    RPCCallFailed(String),
    #[error("RPC call failed: {0}")]
    RPCError(String),
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
}
