#![allow(unused_doc_comments)]
#![no_main]

use std::str::FromStr;
use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::SolValue;
use celestia_types::hash::Hash;
use celestia_types::{AppVersion, MerkleProof};
use risc0_steel::ethereum::EthBlockHeader;
use risc0_steel::{
    ethereum::{EthEvmInput, ETH_SEPOLIA_CHAIN_SPEC},
    Commitment, Contract, EvmEnv, StateDb,
};
use risc0_zkvm::guest::env;
use toolkit::blobstream::{
    compute_ods_width_from_row_proof, BinaryMerkleProof, DaFraud, DaGuestError, DataRootTuple,
    IDAOracle, InputError,
};
use toolkit::journal::Journal;
use toolkit::{
    eds_index_to_ods, BlobIndex, BlobProofData, BlobstreamAttestation,
    BlobstreamAttestationAndRowProof, DaFraudGuestData, SpanSequence,
};
use toolkit::constants::BLOBSTREAM_ADDRESS;

risc0_zkvm::guest::entry!(main);

fn verify_blobstream_attestation(
    blobstream_contract: &Contract<&EvmEnv<StateDb, EthBlockHeader, Commitment>>,
    block_height: u64,
    BlobstreamAttestationAndRowProof {
        blobstream_attestation,
        row_proof,
        row_root_node,
    }: &BlobstreamAttestationAndRowProof,
) {
    let formatted_proof = BinaryMerkleProof::from(blobstream_attestation.proof.clone());

    let blobstream_call = IDAOracle::verifyAttestationCall {
        _tupleRootNonce: U256::from(blobstream_attestation.nonce),
        _tuple: DataRootTuple {
            height: U256::from(block_height),
            dataRoot: B256::from_slice(&blobstream_attestation.data_root),
        },
        _proof: formatted_proof,
    };

    // `verifyAttestation()` returns nothing, discard the return value
    let _blobstream_return = blobstream_contract.call_builder(&blobstream_call).call();

    // TODO: this serialization can be performed on the host side
    let serialized_row_root_node = borsh::to_vec(&row_root_node).unwrap();

    row_proof
        .verify(&serialized_row_root_node, blobstream_attestation.data_root)
        .unwrap();
}

fn verify_span_sequence_inclusion(
    blob_commitment: &SpanSequence,
    row_proof: &MerkleProof,
) -> Result<(), DaGuestError> {
    let eds_width = compute_ods_width_from_row_proof(&row_proof)? * 2;
    let eds_size = eds_width * eds_width;

    let last_share_index = blob_commitment.end_index_eds();

    if last_share_index > eds_size {
        env::log(&format!(
            "invalid blob commitment end index: {} > {}",
            last_share_index, eds_size
        ));
        return Err(DaFraud::ShareIndexOutOfBounds {
            share_index: last_share_index,
            eds_size,
        }
        .into());
    }

    Ok(())
}

fn verify_share_proof(
    blob_commitment: &SpanSequence,
    blobstream_attestation: &BlobstreamAttestation,
    blob_proof_data: &BlobProofData,
) {
    let share_proof = &blob_proof_data.share_proof;

    // Verify that the share proof belongs to the expected Celestia block data root
    share_proof
        .verify(Hash::Sha256(blobstream_attestation.data_root))
        .expect("failed to verify share proof");

    let eds_width = compute_ods_width_from_row_proof(&share_proof.row_proof.proofs[0]).unwrap() * 2;

    // Verify that the share proof covers the indices of the blob commitment:
    // 1. Check that the start index matches
    let share_proof_start_index = blob_proof_data.start_index_ods();
    assert_eq!(
        share_proof_start_index,
        eds_index_to_ods(blob_commitment.start, eds_width),
        "invalid share proof start index",
    );
    // 2. Check that the size matches
    let shares_covered = blob_proof_data.shares_covered();
    assert_eq!(shares_covered, blob_commitment.size);
}

fn check_da_challenge(
    evm_env: &EvmEnv<StateDb, EthBlockHeader, Commitment>,
    serialized_da_guest_data: Vec<u8>,
) -> Result<(), DaGuestError> {
    let DaFraudGuestData {
        index_blob,
        challenged_blob,
        index_blob_data,
        block_proofs,
    } = bincode::deserialize(&serialized_da_guest_data).expect("failed to deserialize guest data");

    // We hardcode the Blobstream contract address as to avoid specifying it on-chain.
    // This way, the Blobstream ID is tied to the guest ID.
    let blobstream_address = Address::from_str(BLOBSTREAM_ADDRESS).expect("invalid blobstream address");
    let blobstream_contract = Contract::new(blobstream_address, &evm_env);

    // Verify the authenticity of all the provided blocks.
    for (block_height, block_proof) in &block_proofs {
        verify_blobstream_attestation(&blobstream_contract, *block_height, &block_proof);
    }

    // If the index blob is the missing blob, verify exclusion immediately.
    if challenged_blob == index_blob {
        // Verify that the index blob is excluded
        return verify_span_sequence_inclusion(
            &index_blob,
            &block_proofs[&index_blob.height].row_proof,
        );
    }

    // To go any further, the index blob data must be present.
    let index_blob_data = index_blob_data.ok_or(InputError::MissingIndexBlobData)?;

    // Verify the share proofs of the index blob
    verify_share_proof(
        &index_blob,
        &block_proofs[&index_blob.height].blobstream_attestation,
        &index_blob_data,
    );
    // Deserialize the index blob
    let app_version =
        AppVersion::from_u64(index_blob_data.app_version).expect("invalid app version");
    let index = BlobIndex::reconstruct_from_raw(index_blob_data.share_proof.shares(), app_version)
        .expect("invalid index");

    // Iterate over the blobs in the index and check if they're the missing blob.
    for blob_commitment in index.blobs {
        if challenged_blob == blob_commitment {
            return verify_span_sequence_inclusion(
                &blob_commitment,
                &block_proofs[&blob_commitment.height].row_proof,
            );
        }
    }

    Err(InputError::ChallengedBlobNotInIndex.into())
}

fn main() {
    // Read the input from the guest environment.
    let input: EthEvmInput = env::read();
    let serialized_da_guest_data: Vec<u8> = env::read_frame();

    // Converts the input into a `EvmEnv` for execution. The `with_chain_spec` method is used
    // to specify the chain configuration. It checks that the state matches the state root in the
    // header provided in the input.
    let evm_env = input.into_env().with_chain_spec(&ETH_SEPOLIA_CHAIN_SPEC);

    match check_da_challenge(&evm_env, serialized_da_guest_data) {
        Ok(()) => panic!("the specified blob is available, DA challenge failed"),
        Err(DaGuestError::Input(err)) => {
            panic!("invalid input: {}", err)
        }
        Err(DaGuestError::Fraud(err)) => env::log(&format!("DA challenge success: {err}")),
    }

    // Commit the block hash and number used when deriving `view_call_env` to the journal.
    let journal = Journal {
        commitment: evm_env.into_commitment(),
    };
    env::commit_slice(&journal.abi_encode());
}
