use std::env;

use axelar_xrpl_relayer::{
    includer::{Broadcaster, RefundManager},
    xrpl_includer::XRPLIncluder,
};

#[tokio::main]
async fn main() {
    let refund_manager_address = env::var("REFUND_MANAGER_ADDRESS")
        .expect("REFUND_MANAGER_ADDRESS environment variable")
        .to_string();
    let includer_secret = env::var("INCLUDER_SECRET")
        .expect("INCLUDER_SECRET environment variable")
        .to_string();

    let xrpl_includer = XRPLIncluder::new(includer_secret, refund_manager_address).await;
    let tx = xrpl_includer
        .refund_manager
        .build_refund_tx("rBgPkze2VmNFutLCPzHs5QQBUSskVRjfjj".to_string(), 1230000)
        .await
        .unwrap();
    println!("{:?}", tx);
    let res = xrpl_includer.broadcaster.broadcast(tx).await.unwrap();
    println!("{:?}", res);
}
