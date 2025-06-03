use bincode::{deserialize, serialize};
use serde::{Deserialize, Serialize};
use crate::{error::FastPayError, message::*};

/// Message types for network communication
#[derive(Serialize, Deserialize)]
pub enum BridgeMessage {
    CrossChainTransferOrder(CrossChainTransferOrder),
    SignedCrossChainTransferOrder(SignedCrossChainTransferOrder),
    CertifiedCrossChainTransferOrder(CertifiedCrossChainTransferOrder),
    CrossShardUpdate(CrossShardCrossChainUpdate),
    Error(String),
}

/// Serialize a message to bytes
pub fn serialize_message(message: &BridgeMessage) -> Vec<u8> {
    serialize(message).expect("Serialization failed")
}

/// Deserialize bytes to a message
pub fn deserialize_message(bytes: &[u8]) -> Result<BridgeMessage, FastPayError> {
    deserialize(bytes).map_err(|_| FastPayError::DeserializationError)
}

/// Serialize an error to a message
pub fn serialize_error(error: &FastPayError) -> Vec<u8> {
    let message = BridgeMessage::Error(format!("{:?}", error));
    serialize_message(&message)
}

/// Helper functions for specific message types
pub fn serialize_transfer_order(order: &CrossChainTransferOrder) -> Vec<u8> {
    serialize_message(&BridgeMessage::CrossChainTransferOrder(order.clone()))
}

pub fn serialize_signed_order(order: &SignedCrossChainTransferOrder) -> Vec<u8> {
    serialize_message(&BridgeMessage::SignedCrossChainTransferOrder(order.clone()))
}

pub fn serialize_certified_order(order: &CertifiedCrossChainTransferOrder) -> Vec<u8> {
    serialize_message(&BridgeMessage::CertifiedCrossChainTransferOrder(order.clone()))
}

pub fn serialize_cross_shard_update(update: &CrossShardCrossChainUpdate) -> Vec<u8> {
    serialize_message(&BridgeMessage::CrossShardUpdate(update.clone()))
}