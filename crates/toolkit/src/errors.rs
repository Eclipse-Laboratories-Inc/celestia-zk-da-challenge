use crate::SpanSequence;
use celestia_types::MerkleProof;

/// An error in the inputs passed to the guest program or in the guest program itself.
/// An error of this type should not lead to the generation of a valid proof.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("invalid number of leaves in proof")]
    InvalidNumberOfLeavesInProof,

    #[error("the blob under challenge is not part of the specified index")]
    ChallengedBlobNotInIndex,

    #[error("missing index blob data")]
    MissingIndexBlobData,

    #[error("first Blobstream attestation nonce != 1")]
    InvalidFirstBlobstreamAttestationNonce,

    #[error("first Blobstream attestation index != 0")]
    InvalidFirstBlobstreamAttestationIndex,
}

/// An error that implies DA fraud.
#[derive(Debug, thiserror::Error)]
pub enum DaFraud {
    #[error("Failed to reconstruct index blob from shares: {0}")]
    FailedIndexBlobReconstruction(#[from] celestia_types::Error),

    #[error("Failed to deserialize index blob: {0}")]
    FailedIndexBlobDeserialization(#[from] bincode::Error),

    #[error("Share index out of bounds: {share_index} > {ods_size}")]
    ShareIndexOutOfBounds { share_index: u32, ods_size: u32 },

    #[error(
        "Block height lower than minimum Blobstream height: {block_height} < {min_block_height}"
    )]
    BlockHeightTooLow {
        block_height: u64,
        min_block_height: u64,
    },

    #[error(
        "Block height higher than current Blobstream height: {block_height} < {max_block_height}"
    )]
    BlockHeightTooHigh {
        block_height: u64,
        max_block_height: u64,
    },

    #[error("Overflow while computing span sequence end: {0:?}")]
    SpanSequenceOverflow(SpanSequence),

    #[error("Sequence of spans is empty: {0:?}")]
    EmptySpanSequence(SpanSequence),
}

#[derive(Debug, thiserror::Error)]
pub enum DaGuestError {
    #[error(transparent)]
    Input(#[from] InputError),
    #[error(transparent)]
    Fraud(#[from] DaFraud),
}

pub fn compute_ods_width_from_row_proof(row_proof: &MerkleProof) -> Result<u32, DaGuestError> {
    if (row_proof.total % 4) != 0 {
        return Err(InputError::InvalidNumberOfLeavesInProof.into());
    }

    let square_size = row_proof.total / 4;
    Ok(square_size as u32)
}
