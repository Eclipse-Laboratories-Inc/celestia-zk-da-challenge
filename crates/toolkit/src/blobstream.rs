use alloy_sol_types::private::{B256, U256};
use alloy_sol_types::sol;
use celestia_types::MerkleProof;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

sol! {
    /// @notice A representation of the Celestia-app namespace ID and its version.
    /// See: https://celestiaorg.github.io/celestia-app/specs/namespace.html
    struct Namespace {
        // The namespace version.
        bytes1 version;
        // The namespace ID.
        bytes28 id;
    }

    /// @notice Namespace Merkle Tree node.
    struct NamespaceNode {
        // Minimum namespace.
        Namespace min;
        // Maximum namespace.
        Namespace max;
        // Node value.
        bytes32 digest;
    }

    /// @notice A tuple of data root with metadata. Each data root is associated
    ///  with a Celestia block height.
    /// @dev `availableDataRoot` in
    ///  https://github.com/celestiaorg/celestia-specs/blob/master/src/specs/data_structures.md#header
    struct DataRootTuple {
        // Celestia block height the data root was included in.
        // Genesis block is height = 0.
        // First queryable block is height = 1.
        uint256 height;
        // Data root.
        bytes32 dataRoot;
    }

    /// @notice Merkle Tree Proof structure.
    struct BinaryMerkleProof {
        // List of side nodes to verify and calculate tree.
        bytes32[] sideNodes;
        // The key of the leaf to verify.
        uint256 key;
        // The number of leaves in the tree
        uint256 numLeaves;
    }

    /// @notice Data Availability Oracle interface.
    interface IDAOracle {
        /// @notice Verify a Data Availability attestation.
        /// @param _tupleRootNonce Nonce of the tuple root to prove against.
        /// @param _tuple Data root tuple to prove inclusion of.
        /// @param _proof Binary Merkle tree proof that `tuple` is in the root at `_tupleRootNonce`.
        /// @return `true` is proof is valid, `false` otherwise.
        function verifyAttestation(uint256 _tupleRootNonce, DataRootTuple memory _tuple, BinaryMerkleProof memory _proof)
            external
            view
            returns (bool);
    }
}

impl Serialize for BinaryMerkleProof {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // helper gets the auto-derive
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Helper<'a> {
            side_nodes: &'a [B256],
            key: &'a U256,
            num_leaves: &'a U256,
        }

        let helper = Helper {
            side_nodes: &self.sideNodes,
            key: &self.key,
            num_leaves: &self.numLeaves,
        };

        helper.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BinaryMerkleProof {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Helper {
            side_nodes: Vec<B256>,
            key: U256,
            num_leaves: U256,
        }

        let helper = Helper::deserialize(deserializer)?;
        Ok(Self {
            sideNodes: helper.side_nodes,
            key: helper.key,
            numLeaves: helper.num_leaves,
        })
    }
}

impl From<MerkleProof> for BinaryMerkleProof {
    fn from(proof: MerkleProof) -> Self {
        // 1.  Vec<Hash> ➜ Vec<B256>
        //
        //     `Hash` in the Lumina crates is an opaque wrapper around
        //     `[u8; 32]`; it implements `AsRef<[u8]>`, so we can copy
        //     the bytes straight into Alloy’s fixed-length `B256`.
        let side_nodes: Vec<B256> = proof
            .aunts
            .iter()
            .map(|h| {
                // `B256::from_slice` takes a `&[u8; 32]` (panics if len ≠ 32).
                B256::from_slice(h.as_ref())
            })
            .collect();

        // 2.  usize ➜ U256
        //
        //     Both `index` (key) and `total` (numLeaves) fit inside
        //     256 bits; just cast through `u64` to be safe on 32-bit
        //     targets and then into `U256`.
        let key = U256::from(proof.index as u64);
        let num_leaves = U256::from(proof.total as u64);

        // 3.  Assemble the ABI struct
        BinaryMerkleProof {
            sideNodes: side_nodes,
            key,
            numLeaves: num_leaves,
        }
    }
}

/// An error in the inputs passed to the guest program or in the guest program itself.
/// An error of this type should not lead to the generation of a valid proof.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("invalid number of leaves in proof")]
    InvalidNumberOfLeavesInProof,

    #[error("the blob under challenge blob is not part of the specified index")]
    ChallengedBlobNotInIndex,

    #[error("missing index blob data")]
    MissingIndexBlobData,
}

/// An error that implies DA fraud.
#[derive(Debug, thiserror::Error)]
pub enum DaFraud {
    #[error("Failed to reconstruct index blob from shares: {0}")]
    FailedIndexBlobReconstruction(#[from] celestia_types::Error),

    #[error("Failed to deserialize index blob: {0}")]
    FailedIndexBlobDeserialization(#[from] bincode::Error),

    #[error("Share index out of bounds: {share_index} > {eds_size}")]
    ShareIndexOutOfBounds{share_index: u32, eds_size: u32},
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
