// SPDX-License-Identifier: MIT
pragma solidity ^0.8.19;

import "forge-std/Script.sol";
import "../src/Verifier.sol";
import "../src/MockGateway.sol";
import "../src/MockBlobstream.sol";
import "../src/ImageID.sol";

contract DeployVerifier is Script {
    // Use the ImageID from the generated methods
    bytes32 constant VERIFIER_IMAGE_ID = ImageID.DA_BRIDGE_ID;
    
    // RISC Zero Verifier addresses
    address constant RISC0_VERIFIER_MAINNET = 0x8EaB2D97Dfce405A1692a21b3ff3A172d593D319;
    address constant RISC0_VERIFIER_SEPOLIA = 0x925d8331ddc0a1F0d96E68CF073DFE1d92b69187;
    
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);
        
        // Deploy mock contracts for testing
        MockGateway mockGateway = new MockGateway();
        MockBlobstream mockBlobstream = new MockBlobstream();
        
        // Choose RISC0 verifier based on chain ID
        address risc0Verifier;
        if (block.chainid == 1) {
            risc0Verifier = RISC0_VERIFIER_MAINNET;
        } else if (block.chainid == 11155111) {
            risc0Verifier = RISC0_VERIFIER_SEPOLIA;
        } else {
            // For local testing, deploy a mock verifier
            risc0Verifier = address(new MockRiscZeroVerifier());
        }
        
        // Deploy the main verifier contract
        Verifier verifier = new Verifier(
            address(mockGateway),
            risc0Verifier,
            address(mockBlobstream),
            VERIFIER_IMAGE_ID,
            VERIFIER_IMAGE_ID
        );
        
        vm.stopBroadcast();
        
        // Log deployment addresses
        console.log("Deployed contracts:");
        console.log("MockGateway:", address(mockGateway));
        console.log("MockBlobstream:", address(mockBlobstream));
        console.log("RISC0 Verifier:", risc0Verifier);
        console.log("Deployed Verifier to", address(verifier));
        console.log("Chain ID:", block.chainid);
    }
}

// Mock RISC Zero Verifier for local testing
contract MockRiscZeroVerifier {
    function verify(
        bytes calldata, // seal
        bytes32,        // imageId
        bytes32         // journalHash
    ) external pure {
        // Always succeeds for testing
    }
} 