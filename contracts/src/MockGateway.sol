// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "./Verifier.sol";

contract MockGateway is IGateway {
    mapping(bytes32 => bool) private canonicalBatches;
    mapping(bytes32 => BatchHeader) private batchHeaders;
    mapping(bytes32 => uint256) private indexBlobHeights;
    
    event BatchSet(bytes32 indexed batchHash, bool isCanonical);
    
    function setBatch(bytes32 batchHash, BatchHeader memory header, bool isCanonical) external {
        canonicalBatches[batchHash] = isCanonical;
        batchHeaders[batchHash] = header;
        emit BatchSet(batchHash, isCanonical);
    }
    
    function setIndexBlobHeight(bytes32 indexBlobHash, uint256 height) external {
        indexBlobHeights[indexBlobHash] = height;
    }
    
    function getIndexBlobHeight(bytes32 indexBlobHash) external view override returns (uint256 indexBlobHeight) {
        return indexBlobHeights[indexBlobHash];
    }
    
    function isCanonicalBatch(bytes32 batchHash) external view override returns (bool isCanonical) {
        return canonicalBatches[batchHash];
    }
    
    function batchHeader(bytes32 batchHash) external view override returns (BatchHeader memory) {
        return batchHeaders[batchHash];
    }
} 