use serde::{ Deserialize, Serialize };
use sha2::{ Digest, Sha512 };
use std::hash::{ Hash };
use ed25519_dalek as dalek;
use ed25519_dalek::{ Signer, Verifier };

use crate::error::FastPayError;

/// Chain identifier for source and destination chains
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize)]
pub struct ChainId(pub u16);

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Pubkey(pub [u8; 32]);

impl Pubkey {
    pub fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Pubkey(bytes)
    }
}

#[derive(Eq, PartialEq, Copy, Clone, Serialize, Deserialize, Debug)]
pub struct Signature(pub dalek::Signature);

impl From<[u8; 64]> for Signature {
    fn from(bytes: [u8; 64]) -> Self {
        let sig = dalek::Signature::from_bytes(&bytes);
        Signature(sig)
    }
}

/// Unique identifier for cross-chain transfers
#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize, PartialOrd, Ord)]
pub struct InteropTxId(pub [u8; 32]);

pub type ShardId = u32;
pub type AuthorityName = Pubkey;
pub struct KeyPair(dalek::SigningKey);

impl KeyPair {
    pub fn from(secret: [u8; 32]) -> Self {
        let signing_key = dalek::SigningKey::from_bytes(&secret);
        KeyPair(signing_key)
    }

    pub fn public(&self) -> Pubkey {
        Pubkey(self.0.verifying_key().to_bytes())
    }
}

/// Cross-chain transfer information
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct CrossChainTransfer {
    pub source_chain: ChainId,
    pub destination_chain: ChainId,
    pub sender: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
    pub token_mint: Pubkey, // Mint address on destination chain
    pub interop_tx_id: InteropTxId,
    pub escrow_account: Pubkey, // Escrow account on source chain
    pub nonce: u64, // Nonce to prevent replay attacks
}

impl CrossChainTransfer {
    pub fn key(&self) -> (Pubkey, InteropTxId) {
        (self.sender, self.interop_tx_id)
    }

    /// Determine which shard should process this transfer
    pub fn shard_id(&self) -> ShardId {
        // Shard based on the first byte of sender address, like in FastPay
        let bytes = self.sender.0.as_ref();
        (bytes[0] as u32) % 16 // 16 shards like in the original
    }
}

/// Implementation of BcsSignable trait for CrossChainTransfer
impl BcsSignable for CrossChainTransfer {}

/// Generation of InteropTxId
impl InteropTxId {
    /// Generate a deterministic InteropTxId from transfer details
    pub fn generate(
        source_chain: ChainId,
        destination_chain: ChainId,
        sender: Pubkey,
        recipient: Pubkey,
        amount: u64,
        token_mint: Pubkey,
        nonce: u64
    ) -> Self {
        let mut hasher = Sha512::new();
        // Include all relevant fields in the hash
        hasher.update(&source_chain.0.to_le_bytes());
        hasher.update(&destination_chain.0.to_le_bytes());
        hasher.update(sender.0.as_ref());
        hasher.update(recipient.0.as_ref());
        hasher.update(&amount.to_le_bytes());
        hasher.update(token_mint.0.as_ref());
        hasher.update(&nonce.to_le_bytes());

        let result = hasher.finalize();
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&result[0..32]);

        InteropTxId(bytes)
    }
}

/// Something that we know how to hash and sign.
pub trait Signable<Hasher> {
    fn write(&self, hasher: &mut Hasher);
}

/// Activate the blanket implementation of `Signable` based on serde and BCS.
/// * We use `serde_name` to extract a seed from the name of structs and enums.
/// * We use `BCS` to generate canonical bytes suitable for hashing and signing.
pub trait BcsSignable: Serialize + serde::de::DeserializeOwned {}

impl<T, Hasher> Signable<Hasher> for T where T: BcsSignable, Hasher: std::io::Write {
    fn write(&self, hasher: &mut Hasher) {
        let name = serde_name::trace_name::<Self>().expect("Self must be a struct or an enum");
        // Note: This assumes that names never contain the separator `::`.
        write!(hasher, "{}::", name).expect("Hasher should not fail");
        bcs::serialize_into(hasher, &self).expect("Message serialization should not fail");
    }
}

impl Signature {
    pub fn new<T>(value: &T, secret: &KeyPair) -> Self where T: Signable<Vec<u8>> {
        let mut message = Vec::new();
        value.write(&mut message);
        let signature = secret.0.sign(&message);
        Signature(signature)
    }

    fn check_internal<T>(&self, value: &T, author: Pubkey) -> Result<(), dalek::SignatureError>
        where T: Signable<Vec<u8>>
    {
        let mut message = Vec::new();
        value.write(&mut message);
        let public_key = dalek::VerifyingKey::from_bytes(&author.0)?;
        public_key.verify(&message, &self.0)
    }

    pub fn check<T>(&self, value: &T, author: Pubkey) -> Result<(), FastPayError>
        where T: Signable<Vec<u8>>
    {
        self.check_internal(value, author).map_err(|error| FastPayError::InvalidSignature {
            error: format!("{} --- from check signature", error),
        })
    }

    fn verify_batch_internal<'a, T, I>(value: &'a T, votes: I) -> Result<(), dalek::SignatureError>
        where T: Signable<Vec<u8>>, I: IntoIterator<Item = &'a (Pubkey, Signature)>
    {
        let mut msg = Vec::new();
        value.write(&mut msg);
        let mut messages: Vec<&[u8]> = Vec::new();
        let mut signatures: Vec<dalek::Signature> = Vec::new();
        let mut public_keys: Vec<dalek::VerifyingKey> = Vec::new();
        for (addr, sig) in votes.into_iter() {
            messages.push(&msg);
            signatures.push(sig.0);
            public_keys.push(dalek::VerifyingKey::from_bytes(&addr.0)?);
        }
        dalek::verify_batch(&messages[..], &signatures[..], &public_keys[..])
    }

    pub fn verify_batch<'a, T, I>(value: &'a T, votes: I) -> Result<(), FastPayError>
        where T: Signable<Vec<u8>>, I: IntoIterator<Item = &'a (Pubkey, Signature)>
    {
        Signature::verify_batch_internal(value, votes).map_err(|error| {
            FastPayError::InvalidSignature {
                error: format!("{} --- from verify signature", error),
            }
        })
    }
}
