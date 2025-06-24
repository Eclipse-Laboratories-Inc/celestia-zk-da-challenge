# Apps

## Publisher

The [`publisher` CLI][publisher], is an example application that sends an off-chain proof request to the RISC Zero zkVM, and publishes DA fraud proofs to verifier contracts.

### Usage

Run the `publisher` with:

```sh
cargo run --bin publisher
```

```text
$ cargo run --bin publisher -- --help

Usage: publisher [OPTIONS] --eth-wallet-private-key <ETH_WALLET_PRIVATE_KEY> --eth-rpc-url <ETH_RPC_URL> --verifier-address <VERIFIER_ADDRESS> --index-blob <INDEX_BLOB> --challenged-blob <CHALLENGED_BLOB>

Options:
      --eth-wallet-private-key <ETH_WALLET_PRIVATE_KEY>
          Ethereum private key
          
          [env: ETH_WALLET_PRIVATE_KEY=]

      --eth-rpc-url <ETH_RPC_URL>
          Ethereum RPC endpoint URL
          
          [env: ETH_RPC_URL=]

      --beacon-api-url <BEACON_API_URL>
          Optional Beacon API endpoint URL
          
          When provided, Steel uses a beacon block commitment instead of the execution block. This allows proofs to be validated using the EIP-4788 beacon roots contract.
          
          [env: BEACON_API_URL=]

      --verifier-address <VERIFIER_ADDRESS>
          Address of the DA fraud proof verifier contract

      --index-blob <INDEX_BLOB>
          Sequence of spans pointing to the index blob

      --challenged-blob <CHALLENGED_BLOB>
          Sequence of spans pointing to the missing blob

  -h, --help
          Print help (see a summary with '-h')
```

[publisher]: ./src/bin/publisher.rs
