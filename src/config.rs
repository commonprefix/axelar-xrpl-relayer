use std::env;

use anyhow::{Context, Result};

pub struct Config {
    pub refund_manager_address: String,
    pub includer_secret: String,
    pub queue_address: String,
    pub gmp_api_url: String,
    pub xrpl_rpc: String,
    pub multisig_address: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            refund_manager_address: env::var("REFUND_MANAGER_ADDRESS")
                .context("Missing REFUND_MANAGER_ADDRESS")?,
            includer_secret: env::var("INCLUDER_SECRET").context("Missing INCLUDER_SECRET")?,
            queue_address: env::var("QUEUE_ADDRESS").context("Missing QUEUE_ADDRESS")?,
            gmp_api_url: env::var("GMP_API").context("Missing GMP_API")?,
            xrpl_rpc: env::var("XRPL_RPC").context("Missing XRPL_RPC")?,
            multisig_address: env::var("MULTISIG_ADDRESS").context("Missing MULTISIG_ADDRESS")?,
        })
    }
}
