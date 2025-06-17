pragma solidity ^0.8.20;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import {IRiscZeroVerifier} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroCheats} from "risc0/test/RiscZeroCheats.sol";
import {Counter} from "../src/Counter.sol";

/// @notice Deployment script for the Counter contract.
/// @dev Use the following environment variable to control the deployment:
///   - ETH_WALLET_PRIVATE_KEY private key of the wallet to be used for deployment.
///   - TOKEN_OWNER to deploy a new ERC 20 token, funding that address with tokens or _alternatively_
///   - TOKEN_CONTRACT to link the Counter to an existing ERC20 token.
///
/// See the Foundry documentation for more information about Solidity scripts.
/// https://book.getfoundry.sh/tutorials/solidity-scripting
contract DeployCounter is Script, RiscZeroCheats {
    function run() external {
        uint256 deployerKey = uint256(vm.envBytes32("ETH_WALLET_PRIVATE_KEY"));

        vm.startBroadcast(deployerKey);

        IRiscZeroVerifier verifier = deployRiscZeroVerifier();

        Counter counter = new Counter(verifier);
        console2.log("Deployed Counter to", address(counter));

        vm.stopBroadcast();
    }
}
