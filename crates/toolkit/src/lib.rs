pub mod blobstream;
pub mod constants;
pub mod errors;
pub mod journal;

use celestia_types::consts::appconsts::SHARE_SIZE;
use celestia_types::nmt::NamespacedHash;
use celestia_types::{AppVersion, Blob, MerkleProof, Share, ShareProof};
use errors::DaFraud;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

/// Commits to a Celestia blob by its position in the Original Data Square (ODS).
/// Note that the start index refers to the ODS, but the Celestia API returns the EDS index
/// when retrieving the blob with `Blob.Get`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SpanSequence {
    /// Block height.
    pub height: u64,
    /// Index of the first share of the blob in the ODS.
    pub start: u32,
    /// Number of shares that make up the blob, ignoring parity shares.
    pub size: u32,
}

impl SpanSequence {
    /// Returns the index of the first share after this blob / sequence of spans in the ODS.
    pub fn end_index_ods(&self) -> Result<u32, DaFraud> {
        if self.size == 0 {
            return Err(DaFraud::EmptySpanSequence(*self));
        }

        self.start
            .checked_add(self.size)
            .ok_or(DaFraud::SpanSequenceOverflow(*self))
    }
}

impl FromStr for SpanSequence {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 3 {
            return Err("Expected format: height:start:size".into());
        }

        let height = parts[0].parse::<u64>().map_err(|_| "Invalid height")?;
        let start = parts[1].parse::<u32>().map_err(|_| "Invalid start")?;
        let size = parts[2].parse::<u32>().map_err(|_| "Invalid size")?;

        Ok(SpanSequence {
            height,
            start,
            size,
        })
    }
}

/// The blob index is a structure that points to other blobs.
/// Its purpose is to commit to multiple blobs with a single blob, enabling to push only one
/// commitment on-chain instead of many.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlobIndex {
    pub blobs: Vec<SpanSequence>,
}

impl BlobIndex {
    pub fn new(blobs: Vec<SpanSequence>) -> Self {
        Self { blobs }
    }

    pub fn reconstruct<'a, I>(shares: I, app_version: AppVersion) -> Result<Self, DaFraud>
    where
        I: IntoIterator<Item = &'a Share>,
    {
        let index_blob = Blob::reconstruct(shares, app_version)?;
        let blob_index: BlobIndex = bincode::deserialize(&index_blob.data)?;

        Ok(blob_index)
    }
    pub fn reconstruct_from_raw<'a, I>(
        raw_shares: I,
        app_version: AppVersion,
    ) -> Result<Self, DaFraud>
    where
        I: IntoIterator<Item = &'a [u8; SHARE_SIZE]>,
    {
        // TODO: implement a reconstruct_from_raw method for Blob in lumina, this is a temporary
        //       workaround.
        let shares: Vec<_> = raw_shares
            .into_iter()
            .map(|raw_share| Share::from_raw(raw_share).expect("invalid share size"))
            .collect();

        let index_blob = Blob::reconstruct(&shares, app_version)?;
        let blob_index: BlobIndex = bincode::deserialize(&index_blob.data)?;

        Ok(blob_index)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlobstreamAttestation {
    pub data_root: [u8; 32],
    pub height: u64,
    pub nonce: u64,
    pub proof: MerkleProof,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlobstreamAttestationAndRowProof {
    pub blobstream_attestation: BlobstreamAttestation,
    pub row_proof: MerkleProof,
    pub row_root_node: NamespacedHash,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BlobProofData {
    pub share_proofs: BTreeMap<u32, ShareProof>,
    pub app_version: u64,
}

/// Returns the start index of the share proof in the ODS.
pub fn share_proof_start_index_ods(share_proof: &ShareProof) -> u32 {
    // Row proofs cover rows + columns of the EDS, so we need to divide by 2 to isolate rows,
    // then by 2 again to ignore parity shares.
    let row_size = share_proof.row_proof.proofs[0].total as u32 / 4;
    let row_index = share_proof.row_proof.proofs[0].index as u32;
    let col_index = share_proof.share_proofs[0].start_idx();

    row_index * row_size + col_index
}

impl BlobProofData {
    pub fn shares(&self) -> impl Iterator<Item = &[u8; SHARE_SIZE]> {
        self.share_proofs
            .values()
            .flat_map(|share_proof| share_proof.shares())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DaChallengeGuestData {
    pub index_blob: SpanSequence,
    pub challenged_blob: SpanSequence,
    pub index_blob_proof_data: Option<BlobProofData>,
    pub block_proofs: BTreeMap<u64, BlobstreamAttestationAndRowProof>,
    /// The attestation for the first Celestia block range covered by the Blobstream
    /// contract. This field is used to determine the lower bound of Celestia block heights
    /// on the current chain.
    pub first_blobstream_attestation: BlobstreamAttestation,
}

impl DaChallengeGuestData {
    pub fn blobstream_attestations(&self) -> impl Iterator<Item = &BlobstreamAttestation> {
        [&self.first_blobstream_attestation].into_iter().chain(
            self.block_proofs
                .values()
                .map(|block_proof| &block_proof.blobstream_attestation),
        )
    }
}

/// Converts an EDS index to an ODS index. Only works for data shares, parity share indexes
/// will not be converted properly.
pub fn eds_index_to_ods(eds_index: u32, eds_width: u32) -> u32 {
    let ods_width = eds_width / 2;

    if eds_index < ods_width {
        eds_index
    } else {
        eds_index / 2
    }
}
