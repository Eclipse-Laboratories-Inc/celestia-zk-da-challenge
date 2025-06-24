pub mod blobstream;
pub mod journal;
pub mod constants;

use crate::blobstream::DaFraud;
use celestia_types::consts::appconsts::SHARE_SIZE;
use celestia_types::nmt::NamespacedHash;
use celestia_types::{AppVersion, Blob, MerkleProof, Share, ShareProof};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::str::FromStr;

/// Commits to a Celestia blob by its position in the Extended Data Square (EDS).
/// Note that the start index refers to the EDS as the Celestia API returns this when retrieving
/// the blob with `Blob.Get`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SpanSequence {
    /// Block height.
    pub height: u64,
    /// Index of the first share of the blob in the EDS.
    pub start: u32,
    /// Number of shares that make up the blob, ignoring parity shares.
    pub size: u32,
}

impl SpanSequence {
    /// Returns the index of the first share after this blob / sequence of spans in the EDS.
    pub fn end_index_eds(&self) -> u32 {
        self.start + self.size
    }

    /// Returns the index of the first share after this blob / sequence of spans in the ODS.
    pub fn end_index_ods(&self, eds_width: u32) -> u32 {
        eds_index_to_ods(self.start, eds_width) + self.size
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

        Ok(SpanSequence { height, start, size })
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
    pub share_proof: ShareProof,
    pub app_version: u64,
}

impl BlobProofData {
    /// Returns the start index of the blob in the ODS.
    pub fn start_index_ods(&self) -> u32 {
        // Row proofs cover rows + columns of the EDS, so we need to divide by 2 to isolate rows,
        // then by 2 again to ignore parity shares.
        let row_size = self.share_proof.row_proof.proofs[0].total as u32 / 4;
        let row_index = self.share_proof.row_proof.proofs[0].index as u32;
        let col_index = self.share_proof.share_proofs[0].start_idx();

        row_index * row_size + col_index
    }

    pub fn shares_covered(&self) -> u32 {
        let mut shares_covered = 0;
        for share_proof in &self.share_proof.share_proofs {
            shares_covered += share_proof.end_idx() - share_proof.start_idx();
        }
        shares_covered
    }

    pub fn end_index(&self) -> u32 {
        let row_size = self.share_proof.row_proof.proofs[0].total as u32;
        let row_index = self
            .share_proof
            .row_proof
            .proofs
            .last()
            .expect("there should always be at least one row proof")
            .index as u32;
        let col_index = self
            .share_proof
            .share_proofs
            .last()
            .expect("there should always be at least one share proof")
            .end_idx();

        row_index * row_size + col_index
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DaFraudGuestData {
    pub index_blob: SpanSequence,
    pub challenged_blob: SpanSequence,
    pub index_blob_data: Option<BlobProofData>,
    pub block_proofs: BTreeMap<u64, BlobstreamAttestationAndRowProof>,
}

/// Converts an EDS index to an ODS index.
pub fn eds_index_to_ods(eds_index: u32, eds_width: u32) -> u32 {
    if eds_index >= eds_width {
        eds_index / 2
    } else {
        eds_index
    }
}
