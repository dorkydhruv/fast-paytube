use fast_core::{ base_types::*, message::*, committee::Committee };
use failure::Error;
use log::{ error, info };
use serde::{ Deserialize, Serialize };
use std::collections::{ BTreeMap, HashMap };
use std::fs::File;
use std::io::BufReader;
use std::net::SocketAddr;
use std::time::{ Duration, Instant };
use structopt::StructOpt;
use tokio::time::sleep;

use crate::network::AuthorityShardClient;

#[derive(Debug, StructOpt)]
pub struct RelayerOpt {
    /// Path to committee configuration file
    #[structopt(long)]
    committee: String,

    /// Source chain RPC URL
    #[structopt(long)]
    source_rpc: String,

    /// Destination chain RPC URL
    #[structopt(long)]
    destination_rpc: String,

    /// Polling interval in milliseconds
    #[structopt(long, default_value = "1000")]
    polling_interval: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct CommitteeConfig {
    authorities: Vec<AuthorityEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct AuthorityEntry {
    name: String,
    host: String,
    port: u16,
    weight: u64,
    num_shards: u32,
}

/// Pending transfer state
struct PendingTransfer {
    order: CrossChainTransferOrder,
    signed_orders: HashMap<AuthorityName, SignedCrossChainTransferOrder>,
    weight: usize,
    start_time: Instant,
}

/// Bridge relayer
pub struct Relayer {
    committee: Committee,
    authority_clients: HashMap<ShardId, Vec<AuthorityShardClient>>,
    pending_transfers: HashMap<InteropTxId, PendingTransfer>,
    _source_rpc: String,
    _destination_rpc: String,
    polling_interval: Duration,
    last_poll: Option<Instant>,
}

impl Relayer {
    /// Create a new relayer
    pub async fn new(
        committee_path: &str,
        _source_rpc: String,
        _destination_rpc: String,
        polling_interval: Duration
    ) -> Result<Self, Error> {
        // Load committee configuration
        let config = load_committee_config(committee_path)?;

        // Create committee
        let mut voting_rights = BTreeMap::new();
        let mut authority_clients = HashMap::new();

        // Create authority clients for each shard
        for entry in &config.authorities {
            let name = decode_authority_name(&entry.name)?;
            let authority_name = name.into();

            // Add to voting rights
            voting_rights.insert(authority_name, entry.weight as usize);

            // Create clients for each shard
            for i in 0..entry.num_shards {
                let shard_id = i as ShardId;
                let port = entry.port + (shard_id as u16);
                let addr = format!("{}:{}", entry.host, port);
                let addr: SocketAddr = addr.parse()?;

                let client = AuthorityShardClient::new(authority_name, addr).await?;

                // Add to clients map
                authority_clients.entry(shard_id).or_insert_with(Vec::new).push(client);
            }
        }

        let committee = Committee::new(voting_rights);
        Ok(Self {
            committee,
            authority_clients,
            pending_transfers: HashMap::new(),
            _source_rpc,
            _destination_rpc,
            polling_interval,
            last_poll: None,
        })
    }

    /// Run the relayer
    pub async fn run(&mut self) -> Result<(), Error> {
        info!("Starting bridge relayer");
        println!("Starting bridge relayer (println)"); // Direct console output

        loop {
            // Poll for new transfers
            println!("Polling source chain..."); // Direct console output
            self.poll_source_chain().await?;

            // Check for completed transfers
            println!("Checking pending transfers..."); // Direct console output
            self.check_pending_transfers().await?;

            // Wait for next polling interval
            sleep(self.polling_interval).await;
        }
    }

    /// Poll source chain for new transfers
    async fn poll_source_chain(&mut self) -> Result<(), Error> {
        // In a real implementation, this would connect to the source chain
        // and look for new escrow events

        let now = Instant::now();
        if
            self.last_poll.is_none() ||
            now.duration_since(self.last_poll.unwrap()) > Duration::from_secs(10)
        {
            self.last_poll = Some(now);

            // Create a dummy transfer
            let interop_tx_id = InteropTxId([1u8; 32]);

            if !self.pending_transfers.contains_key(&interop_tx_id) {
                if !self.pending_transfers.contains_key(&interop_tx_id) {
                    // Create a real signature using the sender's keypair
                    let sender_secret = [2u8; 32];
                    let sender_keypair = KeyPair::from(sender_secret);
                    let sender_pubkey = sender_keypair.public();
                    let first_byte = sender_pubkey.0[0];
                    info!("Sender public key first byte: {}", first_byte);
                    info!("This maps to shard: {}", (first_byte as u32) % 16);
                    let transfer = CrossChainTransfer {
                        source_chain: ChainId(1),
                        destination_chain: ChainId(2),
                        sender: sender_pubkey,
                        recipient: Pubkey([3u8; 32]),
                        amount: 1000,
                        token_mint: Pubkey([4u8; 32]),
                        interop_tx_id,
                        escrow_account: Pubkey([5u8; 32]),
                        nonce: 0,
                    };
                    let signature = Signature::new(&transfer, &sender_keypair);

                    let order = CrossChainTransferOrder {
                        transfer,
                        signature,
                    };

                    info!("Generated dummy transfer with ID: {:?}", interop_tx_id);
                    // Process the transfer
                    self.process_transfer(order).await?;
                }
            }
        }

        Ok(())
    }

    /// Check pending transfers for completion
    async fn check_pending_transfers(&mut self) -> Result<(), Error> {
        let now = Instant::now();
        let mut completed = Vec::new();
        let mut timed_out = Vec::new();

        // Check each pending transfer
        for (id, pending) in &self.pending_transfers {
            info!(
                "Checking pending transfer {:?}, weight: {}/{}",
                id,
                pending.weight,
                self.committee.quorum_threshold()
            );

            // Check if we have a quorum
            if pending.weight >= self.committee.quorum_threshold() {
                info!("Quorum threshold reached, attempting to create certificate");
                // Create a certificate
                if let Some(certificate) = self.create_certificate(pending)? {
                    info!(
                        "Certificate created successfully with {} signatures",
                        certificate.signatures.len()
                    );
                    // Submit to destination chain
                    self.submit_to_destination(&certificate).await?;

                    // Propagate to all authorities
                    self.propagate_to_authorities(&certificate).await?;

                    // Mark as completed
                    completed.push(*id);
                } else {
                    error!("Failed to create certificate despite having enough weight");
                }
            } else if
                // Check for timeout (5 minutes)
                now.duration_since(pending.start_time) > Duration::from_secs(300)
            {
                timed_out.push(*id);
            }
        }

        // Remove completed and timed out transfers
        for id in completed {
            self.pending_transfers.remove(&id);
        }

        for id in timed_out {
            error!("Transfer timed out: {:?}", id);
            self.pending_transfers.remove(&id);
        }

        Ok(())
    }

    /// Process a new transfer
    async fn process_transfer(&mut self, order: CrossChainTransferOrder) -> Result<(), Error> {
        let interop_tx_id = order.transfer.interop_tx_id;

        info!("Starting to process transfer with ID: {:?}", interop_tx_id);
        info!("Current authority clients: {}", self.authority_clients.len());
        info!("Committee voting rights: {} members", self.committee.voting_rights.len());

        // Create a new pending transfer
        let pending = PendingTransfer {
            order: order.clone(),
            signed_orders: HashMap::new(),
            weight: 0,
            start_time: Instant::now(),
        };

        self.pending_transfers.insert(interop_tx_id, pending);

        // Determine which shard should handle this transfer
        let shard_id = get_shard_id(&order.transfer, self.authority_clients.len() as u32); // In a real implementation, this would be based on the transfer

        info!(
            "Processing transfer for shard {}, authority count: {}",
            shard_id,
            self.authority_clients.get(&shard_id).map_or(0, |v| v.len())
        );

        // Get clients for this shard
        if let Some(clients) = self.authority_clients.get(&shard_id) {
            let mut signed_orders = Vec::new();
            // Send to all authorities for this shard
            for client in clients {
                info!("Sending transfer order to authority");
                match client.send_transfer_order(&order).await {
                    Ok(signed_order) => {
                        info!(
                            "Received signed order from authority: {:?} from relayer",
                            signed_order.authority
                        );
                        signed_orders.push(signed_order);
                        info!("Signed order added, current count: {}", signed_orders.len());
                    }
                    Err(e) => {
                        // Log error but continue with other authorities
                        error!("Error sending to authority: {:?}", e);
                    }
                }
            }
            info!("Received {} signed orders from shard {}", signed_orders.len(), shard_id);
            for signed_order in signed_orders {
                self.handle_signed_order(signed_order).await?;
            }
        }

        Ok(())
    }

    /// Handle a signed order from an authority
    async fn handle_signed_order(
        &mut self,
        signed_order: SignedCrossChainTransferOrder
    ) -> Result<(), Error> {
        // Check the signature
        if signed_order.check(&self.committee).is_err() {
            error!("Signature verification failed for order from {:?}", signed_order.authority);
            return Ok(());
        }

        let interop_tx_id = signed_order.value.transfer.interop_tx_id;
        let authority = signed_order.authority;

        // Get the pending transfer
        if let Some(pending) = self.pending_transfers.get_mut(&interop_tx_id) {
            // Add the signed order if not already present
            if !pending.signed_orders.contains_key(&authority) {
                // Add weight
                pending.weight += self.committee.weight(&authority);

                info!(
                    "Added signature from authority {:?}, weight now {}/{}",
                    authority,
                    pending.weight,
                    self.committee.quorum_threshold()
                );

                // Add signed order
                pending.signed_orders.insert(authority, signed_order);
            }
        }

        Ok(())
    }

    /// Create a certificate from a pending transfer
    fn create_certificate(
        &self,
        pending: &PendingTransfer
    ) -> Result<Option<CertifiedCrossChainTransferOrder>, Error> {
        info!("Attempting to create certificate with {} signatures", pending.signed_orders.len());

        // Create a signature aggregator
        let mut aggregator = CrossChainSignatureAggregator::new_unsafe(
            pending.order.clone(),
            &self.committee
        );

        // Track whether we've created a certificate
        let mut certificate = None;

        // Add all signatures
        for (name, signed) in &pending.signed_orders {
            match aggregator.append(*name, signed.signature.clone()) {
                Ok(Some(cert)) => {
                    info!("Certificate created with signature from {:?}", name);
                    certificate = Some(cert);
                    break; // Exit the loop once we have a certificate
                }
                Ok(None) => {
                    info!("Added signature from authority {:?} to aggregator", name);
                }
                Err(e) => {
                    error!("Failed to add signature from authority {:?}: {:?}", name, e);
                    return Err(e.into());
                }
            }
        }
        Ok(certificate)
    }

    /// Submit a certificate to the destination chain
    async fn submit_to_destination(
        &self,
        certificate: &CertifiedCrossChainTransferOrder
    ) -> Result<(), Error> {
        // In a real implementation, this would submit the certificate
        // to the destination chain

        info!(
            "Submitting certificate to destination chain: {:?}",
            certificate.value.transfer.interop_tx_id
        );

        Ok(())
    }

    /// Propagate a certificate to all authorities for cross-shard updates
    async fn propagate_to_authorities(
        &self,
        certificate: &CertifiedCrossChainTransferOrder
    ) -> Result<(), Error> {
        info!("Propagating certificate to {} authority shards", self.authority_clients.len());

        // Send to all authorities (all shards)
        for (shard_id, clients) in &self.authority_clients {
            info!("Sending to shard {} with {} authorities", shard_id, clients.len());
            for client in clients {
                if let Err(e) = client.send_certified_order(certificate).await {
                    error!("Error propagating certificate to authority: {:?}", e);
                } else {
                    info!("Certificate propagated successfully to authority in shard {}", shard_id);
                }
            }
        }

        Ok(())
    }
}

/// Load committee configuration from file
fn load_committee_config(path: &str) -> Result<CommitteeConfig, Error> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let config: CommitteeConfig = serde_json::from_reader(reader)?;
    Ok(config)
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

fn get_shard_id(transfer: &CrossChainTransfer, _num_shards: u32) -> ShardId {
    (transfer.sender.0[0] as u32) % 16
}

/// Run the relayer with the given options
pub async fn run_relayer(opt: RelayerOpt) -> Result<(), Error> {
    // Logger is already initialized in main.rs, don't initialize it again

    info!("Initializing relayer with committee: {}", opt.committee);

    let polling_interval = Duration::from_millis(opt.polling_interval);

    let mut relayer = Relayer::new(
        &opt.committee,
        opt.source_rpc,
        opt.destination_rpc,
        polling_interval
    ).await?;

    relayer.run().await
}
