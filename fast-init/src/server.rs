use fast_core::{
    base_types::*,
    authority::*,
    message::*,
    committee::Committee,
    error::*,
    serialization::*,
};
use failure::Error;
use log::{ error, info };
use serde::{ Deserialize, Serialize };
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{ BufReader };
use std::net::SocketAddr;
use std::sync::{ Arc, Mutex };
use structopt::StructOpt;
use tokio::sync::mpsc;

use crate::network::UdpServer;

#[derive(Debug, StructOpt)]
pub struct BridgeServerOpt {
    /// Path to authority configuration file
    #[structopt(long)]
    config: String,

    /// Host address to listen on
    #[structopt(long, default_value = "127.0.0.1")]
    host: String,

    /// Base port for the authority (each shard uses port+shard_id)
    #[structopt(long)]
    port: u16,

    /// Number of shards to run
    #[structopt(long, default_value = "16")]
    num_shards: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthorityConfig {
    /// Authority name (public key)
    pub name: String,

    /// Path to authority secret key file
    pub secret_key: String,

    /// Committee members file
    pub committee: String,
}

/// Simple escrow verifier that always returns true
pub struct DummyEscrowVerifier;

impl EscrowVerifier for DummyEscrowVerifier {
    fn verify_escrow(&self, _transfer: &CrossChainTransfer) -> Result<bool, FastPayError> {
        Ok(true)
    }
}

/// Run a bridge authority server with the given options
pub async fn run_bridge_server(opt: BridgeServerOpt) -> Result<(), Error> {
    // Load authority configuration
    let config = load_authority_config(&opt.config)?;
    // Load committee configuration
    let committee = load_committee(&config.committee)?;

    // Load authority secret key
    let secret = load_secret_key(&config.secret_key)?;
    // Create authority name from public key
    let name = decode_authority_name(&config.name)?.into();

    // Create escrow verifier
    let escrow_verifier = DummyEscrowVerifier;

    // Create bridge authority state
    let (authority_state, cross_shard_receiver) = BridgeAuthorityState::new(
        name,
        secret,
        committee,
        opt.num_shards,
        escrow_verifier
    );

    // Create shared authority state
    let shared_authority = Arc::new(Mutex::new(authority_state));

    // Create cross-shard handler
    let cross_shard_task = handle_cross_shard_updates(
        shared_authority.clone(),
        cross_shard_receiver
    );

    // Create and run shard servers
    let mut server_tasks = Vec::new();

    for i in 0..opt.num_shards {
        let shard_id = i as ShardId;
        let authority = shared_authority.clone();
        let addr = format!("{}:{}", opt.host, opt.port + (i as u16));
        let addr: SocketAddr = addr.parse()?;

        let server_task = run_shard_server(shard_id, authority, addr);
        server_tasks.push(server_task);
    }

    // Wait for all shard servers to complete
    let _shard_results = futures::future::join_all(server_tasks).await;
    // Await on cross-shard updates
    cross_shard_task.await?;

    Ok(())
}

/// Run a server for a specific shard
async fn run_shard_server(
    shard_id: ShardId,
    authority: Arc<Mutex<BridgeAuthorityState<DummyEscrowVerifier>>>,
    addr: SocketAddr
) -> Result<(), Error> {
    let server = UdpServer::new(addr).await?;

    info!("Starting shard server {} on {}", shard_id, addr);

    server.run(move |data| {
        let authority = authority.clone();

        // Try to deserialize the message
        match deserialize_message(data) {
            Ok(BridgeMessage::CrossChainTransferOrder(order)) => {
                // Handle transfer order
                let mut state = authority.lock().unwrap();
                match state.handle_cross_chain_transfer_order(order, shard_id) {
                    Ok(signed_order) => Some(serialize_signed_order(&signed_order)),
                    Err(e) => Some(serialize_error(&e)),
                }
            }
            Ok(BridgeMessage::CrossShardUpdate(update)) => {
                // Handle cross-shard update
                let mut state = authority.lock().unwrap();
                match state.handle_cross_shard_update(update) {
                    Ok(_) => None, // No response needed
                    Err(e) => Some(serialize_error(&e)),
                }
            }
            Ok(BridgeMessage::CertifiedCrossChainTransferOrder(cert)) => {
                // Handle certified transfer order (propagate to all shards)
                let state = authority.lock().unwrap();
                match state.propagate_certified_transfer(cert) {
                    Ok(_) => None, // No response needed
                    Err(e) => Some(serialize_error(&e)),
                }
            }
            Ok(_) => {
                // Unexpected message type
                None
            }
            Err(_) => {
                // Deserialization error
                None
            }
        }
    }).await?;

    Ok(())
}

/// Handle cross-shard updates
async fn handle_cross_shard_updates(
    authority: Arc<Mutex<BridgeAuthorityState<DummyEscrowVerifier>>>,
    mut receiver: mpsc::UnboundedReceiver<CrossShardCrossChainUpdate>
) -> Result<(), Error> {
    while let Some(update) = receiver.recv().await {
        let mut state = authority.lock().unwrap();
        if let Err(e) = state.handle_cross_shard_update(update) {
            error!("Error handling cross-shard update: {:?}", e);
        }
    }

    Ok(())
}

/// Load authority configuration from file
fn load_authority_config(path: &str) -> Result<AuthorityConfig, Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let config: AuthorityConfig = serde_json::from_reader(reader)?;
    Ok(config)
}

/// Load committee configuration from file
fn load_committee(path: &str) -> Result<Committee, Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let config: serde_json::Value = serde_json::from_reader(reader)?;

    // Parse committee configuration
    let mut voting_rights = BTreeMap::new();

    if let Some(authorities) = config.get("authorities").and_then(|v| v.as_array()) {
        for authority in authorities {
            if
                let (Some(name), Some(weight)) = (
                    authority.get("name").and_then(|v| v.as_str()),
                    authority.get("weight").and_then(|v| v.as_u64()),
                )
            {
                let name = decode_authority_name(name)?;
                voting_rights.insert(name.into(), weight as usize);
            }
        }
    }

    Ok(Committee::new(voting_rights))
}

/// Load secret key from string or file
fn load_secret_key(key_or_path: &str) -> Result<KeyPair, Error> {
    let secret_key_str = if key_or_path.contains('/') && std::path::Path::new(key_or_path).exists() {
        // This looks like a file path, try to read from file
        let file = File::open(key_or_path)?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)?
    } else {
        // This looks like a direct secret key string
        key_or_path.to_string()
    };

    // Parse secret key
    let bytes = hex::decode(secret_key_str.trim())?;
    if bytes.len() != 32 {
        return Err(failure::format_err!("Invalid secret key length"));
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);

    Ok(KeyPair::from(key_bytes))
}

/// Decode authority name from string
fn decode_authority_name(name: &str) -> Result<Pubkey, Error> {
    let bytes = hex::decode(name.trim())?;
    if bytes.len() != 32 {
        return Err(failure::format_err!("Invalid public key length"));
    }

    let mut key_bytes = [0u8; 32];
    key_bytes.copy_from_slice(&bytes);

    Ok(Pubkey(key_bytes))
}
