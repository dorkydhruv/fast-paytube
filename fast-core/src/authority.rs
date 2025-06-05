use super::{ base_types::*, committee::Committee, message::*, error::* };
use std::collections::{ HashMap, HashSet };
use tokio::sync::mpsc;

/// Trait for verifying escrow on source chain
pub trait EscrowVerifier {
    /// Verify the escrow exists and has the correct amount locked
    fn verify_escrow(&self, transfer: &CrossChainTransfer) -> Result<bool, FastPayError>;
}

/// State for a single shard of a bridge authority
pub struct BridgeShardState {
    /// Shard identifier
    pub shard_id: ShardId,

    /// Processed cross-chain transfers (to prevent replay)
    pub processed_transfers: HashSet<InteropTxId>,

    /// Pending cross-chain transfers waiting for certification
    pub pending_transfers: HashMap<InteropTxId, CrossChainTransferOrder>,
}

impl BridgeShardState {
    /// Create a new shard state
    pub fn new(shard_id: ShardId) -> Self {
        Self {
            shard_id,
            processed_transfers: HashSet::new(),
            pending_transfers: HashMap::new(),
        }
    }
}

/// The bridge authority implementation
pub struct BridgeAuthorityState<V: EscrowVerifier> {
    /// The authority's identity
    pub name: AuthorityName,

    /// The authority's keypair
    pub secret: KeyPair,

    /// The committee configuration
    pub committee: Committee,

    /// Total number of shards
    pub number_of_shards: u32,

    /// States for all shards managed by this authority
    pub shard_states: HashMap<ShardId, BridgeShardState>,

    /// Channel for cross-shard communication
    pub cross_shard_sender: mpsc::UnboundedSender<CrossShardCrossChainUpdate>,

    /// Escrow verifier
    pub escrow_verifier: V,
}

impl<V: EscrowVerifier> BridgeAuthorityState<V> {
    /// Create a new bridge authority state with multiple shards
    pub fn new(
        name: AuthorityName,
        secret: KeyPair,
        committee: Committee,
        number_of_shards: u32,
        escrow_verifier: V
    ) -> (Self, mpsc::UnboundedReceiver<CrossShardCrossChainUpdate>) {
        // Create channel for cross-shard communication
        let (cross_shard_sender, cross_shard_receiver) = mpsc::unbounded_channel();

        // Create states for all shards
        let mut shard_states = HashMap::new();
        for i in 0..number_of_shards {
            let shard_id = i as ShardId;
            shard_states.insert(shard_id, BridgeShardState::new(shard_id));
        }

        let state = Self {
            name,
            secret,
            committee,
            number_of_shards,
            shard_states,
            cross_shard_sender,
            escrow_verifier,
        };

        (state, cross_shard_receiver)
    }

    /// Get the shard ID for a transfer based on sender address
    pub fn get_shard_id(&self, transfer: &CrossChainTransfer) -> ShardId {
        let first_byte = transfer.sender.0[0];
        (first_byte as u32) % 16
    }

    /// Check if a transfer belongs to a specific shard
    pub fn in_shard(&self, transfer: &CrossChainTransfer, shard_id: ShardId) -> bool {
        self.get_shard_id(transfer) == shard_id
    }

    /// Handle a cross-chain transfer order for a specific shard
    pub fn handle_cross_chain_transfer_order(
        &mut self,
        order: CrossChainTransferOrder,
        shard_id: ShardId
    ) -> Result<SignedCrossChainTransferOrder, FastPayError> {
        // Verify transfer is in this shard
        if !self.in_shard(&order.transfer, shard_id) {
            return Err(FastPayError::WrongShard {
                err: format!(
                    "Transfer sender {} is not in shard {}",
                    order.transfer.sender.0[0],
                    shard_id
                ),
            });
        }

        // Get the shard state
        let shard_state = self.shard_states
            .get_mut(&shard_id)
            .ok_or(FastPayError::ShardStateNotFound { shard_id: shard_id })?;

        // Verify the transfer order signature
        order.check_signature()?;

        // Check if already processed
        let interop_tx_id = order.transfer.interop_tx_id;
        if shard_state.processed_transfers.contains(&interop_tx_id) {
            return Err(FastPayError::CertificateAlreadyExists);
        }

        // Verify escrow on source chain
        if !self.escrow_verifier.verify_escrow(&order.transfer)? {
            return Err(FastPayError::InvalidTransferAmount {
                error: order.transfer.amount.to_string(),
            });
        }

        // Store the order
        shard_state.pending_transfers.insert(interop_tx_id, order.clone());

        // Sign the order
        let signed_order = SignedCrossChainTransferOrder::new(order, self.name, &self.secret);

        Ok(signed_order)
    }

    /// Handle a cross-shard update
    pub fn handle_cross_shard_update(
        &mut self,
        update: CrossShardCrossChainUpdate
    ) -> Result<(), FastPayError> {
        // Get the shard state
        let shard_state = self.shard_states
            .get_mut(&update.shard_id)
            .ok_or(FastPayError::ShardStateNotFound { shard_id: update.shard_id })?;

        // Verify the certificate
        update.transfer_certificate.check(&self.committee)?;

        // Mark as processed
        let interop_tx_id = update.transfer_certificate.value.transfer.interop_tx_id;
        shard_state.processed_transfers.insert(interop_tx_id);

        // Remove from pending if present
        shard_state.pending_transfers.remove(&interop_tx_id);

        Ok(())
    }

    /// Propagate a certified transfer to all shards
    pub fn propagate_certified_transfer(
        &self,
        certificate: CertifiedCrossChainTransferOrder
    ) -> Result<(), FastPayError> {
        // Verify the certificate
        certificate.check(&self.committee)?;

        // Broadcast to all shards
        for shard_id in self.shard_states.keys() {
            let update = CrossShardCrossChainUpdate {
                shard_id: *shard_id,
                transfer_certificate: certificate.clone(),
            };

            self.cross_shard_sender.send(update).map_err(|_| FastPayError::ConfigurationError {
                error: "Failed to send cross-shard update".to_string(),
            })?;
        }

        Ok(())
    }
}

pub struct DummyEscrowVerifier;

impl EscrowVerifier for DummyEscrowVerifier {
    fn verify_escrow(&self, _transfer: &CrossChainTransfer) -> Result<bool, FastPayError> {
        // Dummy implementation always returns true
        // For a real implementation, this would check the source chain or some mechanism that verifies the escrow
        Ok(true)
    }
}
