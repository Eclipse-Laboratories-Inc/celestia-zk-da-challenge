// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

// Interface for RISC Zero verifier - defined inline to avoid import issues with sol! macro
interface IRiscZeroVerifier {
    function verify(bytes calldata seal, bytes32 imageId, bytes32 journalHash) external view;
}

interface IGateway {
    struct BatchHeader {
        bytes32 prevBatchHash;
        bytes32 indexBlobHash;
        uint64 indexBlobHeight;
    }
    
    function getIndexBlobHeight(bytes32 indexBlobHash) external view returns (uint256 indexBlobHeight);
    function isCanonicalBatch(bytes32 batchHash) external view returns (bool isCanonical);
    function batchHeader(bytes32 batchHash) external view returns (BatchHeader memory batchHeader);
}

interface IBlobstream {
    struct BlobstreamProof {
        uint256 height;
        bytes32 dataRoot;
        bytes32[] sideNodes;
        uint256 key;
        uint256 numLeaves;
    }

    function verifyDataRootTuple(BlobstreamProof calldata proof) external view returns (bool verified);
    
    // Proves a specific Celestia block's data root (at height) is included in a larger merkle tree of data roots that has been committed to and attested by Celestia validators.
    function verifyAttestation(
        uint256 tupleRootNonce,
        BlobstreamProof calldata proof
    ) external view returns (bool verified);
}

struct BlobCommitment {
    uint256 commitment;
    uint256 block_height;
}

struct IndexBlob {
    uint256 namespace;
    BlobCommitment[] commitments;
}

struct Commitment {
    uint256 commitment;
    uint256 blockHeight;
    uint256 namespace;
    bool exists;
}

struct ChallengeProof {
    bytes seal;
    bytes32 imageId;
    IBlobstream.BlobstreamProof blobstreamProof;
    bytes32 dataRootTupleRoot;
}

contract Verifier {
    IGateway public immutable gateway;
    IRiscZeroVerifier public immutable risc0Verifier;
    IBlobstream public immutable blobstream;
    bytes32 public immutable INDEX_BLOB_EXCLUSION_IMAGE_ID;
    bytes32 public immutable BLOB_COMMITMENT_EXCLUSION_IMAGE_ID;
    
    event IndexBlobChallenged(bytes32 indexed indexBlobHash);
    event BlobCommitmentChallenged(bytes32 indexed indexBlobHash, bytes32 indexed blobCommitmentHash);
    
    error NotCanonicalBatch();
    error InvalidProof();
    error HeightMismatch();
    error InvalidBlobstreamProof();
    error InvalidImageId();
    
    constructor(
        address _gateway, 
        address _risc0Verifier, 
        address _blobstream,
        bytes32 _indexBlobExclusionImageId,
        bytes32 _blobCommitmentExclusionImageId
    ) {
        gateway = IGateway(_gateway);
        risc0Verifier = IRiscZeroVerifier(_risc0Verifier);
        blobstream = IBlobstream(_blobstream);
        INDEX_BLOB_EXCLUSION_IMAGE_ID = _indexBlobExclusionImageId;
        BLOB_COMMITMENT_EXCLUSION_IMAGE_ID = _blobCommitmentExclusionImageId;
    }
    
    /// Proves the entire index blob is missing from Celestia
    /// ZK proof validates the Steel commitment from the guest program
    function challengeIndexBlob(
        bytes32 indexBlobHash,
        bytes calldata journalData,
        ChallengeProof calldata proof
    ) external returns (bool) {
        if (proof.imageId != INDEX_BLOB_EXCLUSION_IMAGE_ID) {
            revert InvalidImageId();
        }
        
        _validateChallenge(indexBlobHash, proof);
        
        // Verify the proof with the journal data from the guest program
        _verifyProof(proof.seal, proof.imageId, journalData);
        
        emit IndexBlobChallenged(indexBlobHash);
        return true;
    }
    
    function challengeBlobCommitment(
        bytes32 indexBlobHash,
        bytes32 blobCommitmentHash,
        ChallengeProof calldata proof
    ) external returns (bool) {
        if (proof.imageId != BLOB_COMMITMENT_EXCLUSION_IMAGE_ID) {
            revert InvalidImageId();
        }

        _validateChallenge(indexBlobHash, proof);

        bytes memory expectedJournal = abi.encode(indexBlobHash, blobCommitmentHash);
        _verifyProof(proof.seal, proof.imageId, expectedJournal);
        
        emit BlobCommitmentChallenged(indexBlobHash, blobCommitmentHash);
        return true;
    }
    
    function _validateChallenge(
        bytes32 indexBlobHash,
        ChallengeProof calldata proof
    ) internal view {
        uint256 indexBlobHeight = gateway.getIndexBlobHeight(indexBlobHash);

        if (indexBlobHeight == 0) {
            revert NotCanonicalBatch();
        }
        
        if (indexBlobHeight != proof.blobstreamProof.height) {
            revert HeightMismatch();
        }
        
        if (!blobstream.verifyAttestation(0, proof.blobstreamProof)) {
            revert InvalidBlobstreamProof();
        }
    }
    
    function _verifyProof(bytes memory seal, bytes32 imageId, bytes memory expectedJournal) internal view {
        try risc0Verifier.verify(seal, imageId, sha256(expectedJournal)) {
            // Proof verified successfully
        } catch {
            revert InvalidProof();
        }
    }
    
    function getGatewayAddress() external view returns (address) {
        return address(gateway);
    }
    
    function getRisc0VerifierAddress() external view returns (address) {
        return address(risc0Verifier);
    }
    
    function getBlobstreamAddress() external view returns (address) {
        return address(blobstream);
    }
    
    function getIndexBlobExclusionImageId() external view returns (bytes32) {
        return INDEX_BLOB_EXCLUSION_IMAGE_ID;
    }
    
    function getBlobCommitmentExclusionImageId() external view returns (bytes32) {
        return BLOB_COMMITMENT_EXCLUSION_IMAGE_ID;
    }
} 