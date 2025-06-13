pragma solidity ^0.8.20;

import {IRiscZeroVerifier} from "risc0/IRiscZeroVerifier.sol";
import {Steel} from "risc0/steel/Steel.sol";
import {ICounter} from "./ICounter.sol";
import {ImageID} from "./ImageID.sol";

/// @title Counter
/// @notice Implements a counter that increments based on off-chain Steel proofs submitted to this contract.
/// @dev The contract interacts with ERC-20 tokens, using Steel proofs to verify that an account holds at least 1 token
/// before incrementing the counter. This contract leverages RISC0-zkVM for generating and verifying these proofs.
contract Counter is ICounter {
    /// @notice Image ID of the only zkVM binary to accept verification from.
    bytes32 public constant imageID = ImageID.DA_CHALLENGE_GUEST_ID;

    /// @notice RISC Zero verifier contract address.
    IRiscZeroVerifier public immutable verifier;

    /// @notice Address of the ERC-20 token contract.
    address public immutable tokenContract;

    /// @notice Counter to track the number of successful verifications.
    uint256 public counter;

    /// @notice Journal that is committed to by the guest.
    struct Journal {
        Steel.Commitment commitment;
        address blobstreamContract;
    }

    /// @notice Initialize the contract, binding it to a specified RISC Zero verifier and ERC-20 token address.
    constructor(IRiscZeroVerifier _verifier) {
        verifier = _verifier;
        counter = 0;
    }

    /// @inheritdoc ICounter
    function increment(bytes calldata journalData, bytes calldata seal) external {
        // Decode and validate the journal data
        Journal memory journal = abi.decode(journalData, (Journal));
        require(Steel.validateCommitment(journal.commitment), "Invalid commitment");

        // Verify the proof
        bytes32 journalHash = sha256(journalData);
        verifier.verify(seal, imageID, journalHash);

        counter += 1;
    }

    /// @inheritdoc ICounter
    function get() external view returns (uint256) {
        return counter;
    }
}
