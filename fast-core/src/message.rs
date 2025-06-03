use crate::fp_ensure;

use super::{base_types::*, committee::Committee, error::*};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

#[derive(Eq, Clone, Debug, Serialize, Deserialize)]
pub struct CrossChainTransferOrder {
    pub transfer: CrossChainTransfer,
    pub signature: Signature,
}

#[derive(Eq, Clone, Debug, Serialize, Deserialize)]
pub struct SignedCrossChainTransferOrder {
    pub value: CrossChainTransferOrder,
    pub authority: AuthorityName,
    pub signature: Signature,
}

#[derive(Eq, Clone, Debug, Serialize, Deserialize)]
pub struct CertifiedCrossChainTransferOrder {
    pub value: CrossChainTransferOrder,
    pub signatures: Vec<(AuthorityName, Signature)>,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct CrossChainRedeemTransaction {
    pub transfer_certificate: CertifiedCrossChainTransferOrder,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct CrossChainConfirmationOrder {
    pub transfer_certificate: CertifiedCrossChainTransferOrder,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct CrossShardCrossChainUpdate {
    pub shard_id: ShardId,
    pub transfer_certificate: CertifiedCrossChainTransferOrder,
}

impl Hash for CrossChainTransferOrder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.transfer.hash(state);
    }
}

impl PartialEq for CrossChainTransferOrder {
    fn eq(&self, other: &Self) -> bool {
        self.transfer == other.transfer
    }
}

impl Hash for SignedCrossChainTransferOrder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
        self.authority.hash(state);
    }
}

impl PartialEq for SignedCrossChainTransferOrder {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value && self.authority == other.authority
    }
}

impl Hash for CertifiedCrossChainTransferOrder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
        self.signatures.len().hash(state);
        for (name, _) in self.signatures.iter() {
            name.hash(state);
        }
    }
}

impl PartialEq for CertifiedCrossChainTransferOrder {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value &&
            self.signatures.len() == other.signatures.len() &&
            self.signatures
                .iter()
                .map(|(name, _)| name)
                .eq(other.signatures.iter().map(|(name, _)| name))
    }
}

impl CrossChainTransferOrder {
    pub fn new(transfer: CrossChainTransfer, secret: &KeyPair) -> Self {
        let signature = Signature::new(&transfer, secret);
        Self {
            transfer,
            signature,
        }
    }

    pub fn check_signature(&self) -> Result<(), FastPayError> {
        self.signature.check(&self.transfer, self.transfer.sender)
    }
}

impl SignedCrossChainTransferOrder {
    /// Use signing key to create a signed object.
    pub fn new(value: CrossChainTransferOrder, authority: AuthorityName, secret: &KeyPair) -> Self {
        let signature = Signature::new(&value.transfer, secret);
        Self {
            value,
            authority,
            signature,
        }
    }

    /// Verify the signature and return the non-zero voting right of the authority.
    pub fn check(&self, committee: &Committee) -> Result<usize, FastPayError> {
        self.value.check_signature()?;
        let weight = committee.weight(&self.authority);
        fp_ensure!(weight > 0, FastPayError::UnknownSigner);
        self.signature.check(&self.value.transfer, self.authority)?;
        Ok(weight)
    }
}

pub struct CrossChainSignatureAggregator<'a> {
    committee: &'a Committee,
    weight: usize,
    used_authorities: HashSet<AuthorityName>,
    partial: CertifiedCrossChainTransferOrder,
}

impl<'a> CrossChainSignatureAggregator<'a> {
    /// Start aggregating signatures for the given value into a certificate.
    pub fn try_new(value: CrossChainTransferOrder, committee: &'a Committee) -> Result<Self, FastPayError> {
        value.check_signature()?;
        Ok(Self::new_unsafe(value, committee))
    }

    /// Same as try_new but we don't check the order.
    pub fn new_unsafe(value: CrossChainTransferOrder, committee: &'a Committee) -> Self {
        Self {
            committee,
            weight: 0,
            used_authorities: HashSet::new(),
            partial: CertifiedCrossChainTransferOrder {
                value,
                signatures: Vec::new(),
            },
        }
    }

    /// Try to append a signature to a (partial) certificate. Returns Some(certificate) if a quorum was reached.
    pub fn append(
        &mut self,
        authority: AuthorityName,
        signature: Signature
    ) -> Result<Option<CertifiedCrossChainTransferOrder>, FastPayError> {
        signature.check(&self.partial.value.transfer, authority)?;
        // Check that each authority only appears once.
        fp_ensure!(
            !self.used_authorities.contains(&authority),
            FastPayError::CertificateAuthorityReuse
        );
        self.used_authorities.insert(authority);
        // Update weight.
        let voting_rights = self.committee.weight(&authority);
        fp_ensure!(voting_rights > 0, FastPayError::UnknownSigner);
        self.weight += voting_rights;
        // Update certificate.
        self.partial.signatures.push((authority, signature));

        if self.weight >= self.committee.quorum_threshold() {
            Ok(Some(self.partial.clone()))
        } else {
            Ok(None)
        }
    }
}

impl CertifiedCrossChainTransferOrder {
    pub fn key(&self) -> (Pubkey, InteropTxId) {
        let transfer = &self.value.transfer;
        (transfer.sender, transfer.interop_tx_id)
    }

    /// Verify the certificate.
    pub fn check(&self, committee: &Committee) -> Result<(), FastPayError> {
        // Check the quorum.
        let mut weight = 0;
        let mut used_authorities = HashSet::new();
        for (authority, _) in self.signatures.iter() {
            // Check that each authority only appears once.
            fp_ensure!(
                !used_authorities.contains(authority),
                FastPayError::CertificateAuthorityReuse
            );
            used_authorities.insert(*authority);
            // Update weight.
            let voting_rights = committee.weight(authority);
            fp_ensure!(voting_rights > 0, FastPayError::UnknownSigner);
            weight += voting_rights;
        }
        fp_ensure!(weight >= committee.quorum_threshold(), FastPayError::CertificateRequiresQuorum);
        // All what is left is checking signatures!
        let inner_sig = (self.value.transfer.sender, self.value.signature);
        Signature::verify_batch(
            &self.value.transfer,
            std::iter::once(&inner_sig).chain(&self.signatures)
        )
    }
}