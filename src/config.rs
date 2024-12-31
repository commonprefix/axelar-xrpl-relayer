use std::env;

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub refund_manager_address: String,
    pub includer_secret: String,
    pub queue_address: String,
    pub gmp_api_url: String,
    pub payload_cache: String,
    pub xrpl_rpc: String,
    pub xrpl_multisig: String,
    pub xrpl_gateway_address: String,
    pub xrpl_multisig_prover_address: String,
    pub redis_server: String,
    pub payload_cache_auth_token: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            refund_manager_address: env::var("REFUND_MANAGER_ADDRESS")
                .context("Missing REFUND_MANAGER_ADDRESS")?,
            includer_secret: env::var("INCLUDER_SECRET").context("Missing INCLUDER_SECRET")?,
            queue_address: env::var("QUEUE_ADDRESS").context("Missing QUEUE_ADDRESS")?,
            gmp_api_url: env::var("GMP_API").context("Missing GMP_API")?,
            payload_cache: env::var("PAYLOAD_CACHE").context("Missing PAYLOAD_CACHE")?,
            xrpl_rpc: env::var("XRPL_RPC").context("Missing XRPL_RPC")?,
            xrpl_multisig: env::var("XRPL_MULTISIG").context("Missing XRPL_MULTISIG")?,
            xrpl_gateway_address: env::var("XRPL_GATEWAY_ADDRESS")
                .context("Missing XRPL_GATEWAY_ADDRESS")?,
            xrpl_multisig_prover_address: env::var("XRPL_MULTISIG_PROVER_ADDRESS")
                .context("Missing XRPL_MULTISIG_PROVER_ADDRESS")?,
            redis_server: env::var("REDIS_SERVER").context("Missing REDIS_SERVER")?,
            payload_cache_auth_token: env::var("PAYLOAD_CACHE_AUTH_TOKEN").context("Missing PAYLOAD_CACHE_AUTH_TOKEN")?,
        })
    }
}
