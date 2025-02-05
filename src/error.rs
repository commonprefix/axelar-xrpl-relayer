use thiserror::Error;

#[derive(Error, Debug)]
pub enum IncluderError {
    #[error("Connection failed")]
    Connection,
    #[error("RPC call failed: {0}")]
    RPCError(String),
    #[error("Failed to consume queue: {0}")]
    ConsumerError(String),
    #[error("Irrelevant task")]
    IrrelevantTask,
    #[error("Generic error: {0}")]
    GenericError(String),
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
    #[error("Generic error: {0}")]
    GenericError(String),
}

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
}

#[derive(Error, Debug)]
pub enum GmpApiError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),
    #[error("GMP API Request failed: {0}")]
    RequestFailed(String),
    #[error("GMP API Returned Error: {0}")]
    ErrorResponse(String),
    #[error("Failed to parse response from GMP API: {0}")]
    InvalidResponse(String),
    #[error("Invalid Request: {0}")]
    InvalidRequest(String),
    #[error("Generic error: {0}")]
    GenericError(String),
}

#[derive(Error, Debug)]
pub enum IngestorError {
    #[error("Failed to post event on GMP API: {0}")]
    PostEventError(String),
    #[error("Failed with retriable error: {0}")]
    RetriableError(String),
    #[error("Irrelevant task")]
    IrrelevantTask,
    #[error("Failed to parse data: {0}")]
    ParseError(String),
    #[error("Unsupported transaction: {0}")]
    UnsupportedTransaction(String),
    #[error("Generic error: {0}")]
    GenericError(String),
}

#[derive(Error, Debug)]
pub enum DistributorError {
    #[error("Generic error: {0}")]
    GenericError(String),
}
