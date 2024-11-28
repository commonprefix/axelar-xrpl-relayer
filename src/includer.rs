// parse RMQ events (amplifier tasks) and post txs XRPL chain
use crate::{
    error::{BroadcasterError, RefundManagerError},
    queue::Queue,
};

pub trait RefundManager {
    async fn build_refund_tx(
        &self,
        recipient: String,
        amount: u64,
    ) -> Result<String, RefundManagerError>; // returns signed tx_blob
}

pub trait Broadcaster {
    async fn broadcast(&self, tx_blob: String) -> Result<String, BroadcasterError>;
}

pub struct Includer<B, C, R>
where
    B: Broadcaster,
    R: RefundManager,
{
    pub queue: Queue,
    pub chain_client: C,
    pub broadcaster: B,
    pub refund_manager: R,
}
