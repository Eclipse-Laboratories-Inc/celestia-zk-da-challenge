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
    
    // Real Blobstream contract addresses for different networks
    address constant REAL_BLOBSTREAM_MAINNET = 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0;
    address constant REAL_BLOBSTREAM_SEPOLIA = 0x9fE46736679d2D9a65F0992F2272dE9f3c7fa6e0; // Update if different
    
    // RISC Zero Verifier addresses
    address constant RISC0_VERIFIER_MAINNET = 0x8EaB2D97Dfce405A1692a21b3ff3A172d593D319;
    address constant RISC0_VERIFIER_SEPOLIA = 0x925d8331ddc0a1F0d96E68CF073DFE1d92b69187;
    
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);
        
        // Deploy MockGateway (needed for all deployments)
        MockGateway mockGateway = new MockGateway();
        
        // Choose RISC0 verifier and Blobstream based on chain ID
        address risc0Verifier;
        address blobstream;
        bool isRealNetwork = false;
        
        if (block.chainid == 1) {
            // Mainnet
            risc0Verifier = RISC0_VERIFIER_MAINNET;
            blobstream = REAL_BLOBSTREAM_MAINNET;
            isRealNetwork = true;
        } else if (block.chainid == 11155111) {
            // Sepolia
            risc0Verifier = RISC0_VERIFIER_SEPOLIA;
            blobstream = REAL_BLOBSTREAM_SEPOLIA;
            isRealNetwork = true;
        } else {
            // Local testing - deploy mocks
            risc0Verifier = address(new MockRiscZeroVerifier());
            MockBlobstream mockBlobstream = new MockBlobstream();
            blobstream = address(mockBlobstream);
        }
        
        // Deploy the main verifier contract
        Verifier verifier = new Verifier(
            address(mockGateway),
            risc0Verifier,
            blobstream,
            VERIFIER_IMAGE_ID,
            VERIFIER_IMAGE_ID
        );
        
        vm.stopBroadcast();
        
        // Log deployment addresses
        console.log("Deployed contracts:");
        console.log("MockGateway:", address(mockGateway));
        if (isRealNetwork) {
            console.log("Real Blobstream (existing):", blobstream);
        } else {
            console.log("MockBlobstream:", blobstream);
        }
        console.log("RISC0 Verifier:", risc0Verifier);
        console.log("Deployed Verifier to", address(verifier));
        console.log("Chain ID:", block.chainid);
        console.log("Network type:", isRealNetwork ? "Real" : "Test/Local");
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