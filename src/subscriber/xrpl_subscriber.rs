use anyhow::anyhow;
use xrpl_api::{AccountTransaction, RequestPagination};
use xrpl_types::AccountId;

use crate::subscriber::TransactionPoller;

pub struct XrplSubscriber {
    client: xrpl_http_client::Client,
    latest_ledger: i64,
}

impl XrplSubscriber {
    pub async fn new(url: &str) -> Self {
        let client = xrpl_http_client::Client::builder().base_url(url).build();
        XrplSubscriber {
            client,
            latest_ledger: 6920611,
        }
    }
}

impl TransactionPoller for XrplSubscriber {
    type Transaction = AccountTransaction;

    async fn poll(
        &mut self,
        account_id: AccountId,
    ) -> Result<Vec<Self::Transaction>, anyhow::Error> {
        let request = xrpl_api::AccountTxRequest {
            account: account_id.to_address(),
            forward: Some(true),
            ledger_index_min: Some((self.latest_ledger + 1).to_string()),
            ledger_index_max: Some((-1).to_string()),
            pagination: RequestPagination {
                limit: Some(1),
                ..Default::default()
            },
            ..Default::default()
        };
        let res = self.client.call(request).await;
        println!("res: {:?}", res);

        let response = res.map_err(|e| anyhow!("Error getting txs: {:?}", e.to_string()))?;
        self.latest_ledger = response.ledger_index_max.into();
        Ok(response.transactions)
    }
}
