use failure::Fail;
use serde::{Deserialize, Serialize};

use crate::base_types::CrossChainTransfer;

#[macro_export]
macro_rules! fp_bail {
    ($e:expr) => {
        return Err($e);
    };
}

#[macro_export(local_inner_macros)]
macro_rules! fp_ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            fp_bail!($e);
        }
    };
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize, Fail, Hash)]
/// Custom error type for FastPay.
pub enum FastPayError {
    // Signature verification
    #[fail(display = "Signature is not valid: {}", error)]
    InvalidSignature { error: String },
    #[fail(display = "Value was not signed by a known authority")]
    UnknownSigner,
    // Certificate verification
    #[fail(display = "Signatures in a certificate must form a quorum")]
    CertificateRequiresQuorum,
    // Transfer processing
    #[fail(display = "Transfers must have positive amount")]
    IncorrectTransferAmount,
    #[fail(
        display = "Cannot initiate transfer while a transfer order is still pending confirmation: {:?}",
        pending_confirmation
    )]
    PreviousTransferMustBeConfirmedFirst { pending_confirmation: CrossChainTransfer },
    #[fail(display = "Transfer order was processed but no signature was produced by authority")]
    ErrorWhileProcessingTransferOrder,
    #[fail(
        display = "An invalid answer was returned by the authority while requesting a certificate"
    )]
    ErrorWhileRequestingCertificate,
    #[fail(display = "Certificate already exists for this transfer")]
    CertificateAlreadyExists,
    // Synchronization validation
    #[fail(display = "Transaction index must increase by one")]
    UnexpectedTransactionIndex,
    #[fail(display = "Configuration error: {}", error)]
    ConfigurationError { error: String },
    // Account access
    #[fail(display = "No certificate for this account and sequence number")]
    CertificateNotfound,
    #[fail(display = "Unknown sender's account")]
    UnknownSenderAccount,
    #[fail(display = "Signatures in a certificate must be from different authorities.")]
    CertificateAuthorityReuse,
    #[fail(display = "Sequence numbers above the maximal value are not usable for transfers.")]
    InvalidSequenceNumber,
    #[fail(display = "Sequence number overflow.")]
    SequenceOverflow,
    #[fail(display = "Sequence number underflow.")]
    SequenceUnderflow,
    #[fail(display="Invalid transfer amount: {}.", error)]
    InvalidTransferAmount { error: String },
    #[fail(display = "Wrong shard used.")]
    WrongShard,
    #[fail(display = "Invalid cross shard update.")]
    InvalidCrossShardUpdate,
    #[fail(display = "Cannot deserialize.")]
    InvalidDecoding,
    #[fail(display = "Unexpected message.")]
    UnexpectedMessage,
    #[fail(display = "Network error while querying service: {:?}.", error)]
    ClientIoError { error: String },
    #[fail(display = "Deserialization error occurred")]
    DeserializationError,
    #[fail(display = "Communication error with authority")]
    CommunicationError,
}
