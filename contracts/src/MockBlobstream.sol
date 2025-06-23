// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "./Verifier.sol";

contract MockBlobstream is IBlobstream {
    mapping(bytes32 => bool) private validProofs;
    mapping(bytes32 => bool) private validAttestations;
    
    event ProofValiditySet(bytes32 indexed proofHash, bool valid);
    event AttestationValiditySet(bytes32 indexed attestationHash, bool valid);
    
    function setProofValidity(BlobstreamProof calldata proof, bool valid) external {
        bytes32 proofHash = keccak256(abi.encode(proof));
        validProofs[proofHash] = valid;
        emit ProofValiditySet(proofHash, valid);
    }
    
    function setAttestationValidity(uint256 tupleRootNonce, BlobstreamProof calldata proof, bool valid) external {
        bytes32 attestationHash = keccak256(abi.encode(tupleRootNonce, proof));
        validAttestations[attestationHash] = valid;
        emit AttestationValiditySet(attestationHash, valid);
    }
    
    function verifyDataRootTuple(BlobstreamProof calldata proof) external view override returns (bool) {
        bytes32 proofHash = keccak256(abi.encode(proof));
        return validProofs[proofHash];
    }
    
    function verifyAttestation(
        uint256 tupleRootNonce,
        BlobstreamProof calldata proof
    ) external view override returns (bool) {
        bytes32 attestationHash = keccak256(abi.encode(tupleRootNonce, proof));
        return validAttestations[attestationHash];
    }
} 