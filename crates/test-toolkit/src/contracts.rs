use alloy::core::sol;

sol! {
    #[sol(rpc)]
    /// interface subset we need
    contract Blobstream0 {
        function latestHeight() external view returns (uint64);

        /// emitted when a Celestia ExtendedHeader is accepted
        event HeadUpdate(uint64 blockNumber, bytes32 headerHash);
    }
}

sol!(
    #[sol(rpc)]
    Counter,
    "../../out/Counter.sol/Counter.json"
);
