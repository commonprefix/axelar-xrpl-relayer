use axelar_xrpl_relayer::{
    includer::{Broadcaster, RefundManager},
    xrpl_includer::XRPLIncluder,
};

#[tokio::main]
async fn main() {
    let xrpl_includer = XRPLIncluder::new().await;
    let tx = xrpl_includer
        .refund_manager
        .build_refund_tx("rBgPkze2VmNFutLCPzHs5QQBUSskVRjfjj".to_string(), 1230000)
        .await
        .unwrap();
    println!("{:?}", tx);
    let res = xrpl_includer.broadcaster.broadcast(tx).await.unwrap();
    println!("{:?}", res);
}
