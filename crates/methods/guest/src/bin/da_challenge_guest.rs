#![allow(unused_doc_comments)]
#![no_main]

use alloy_primitives::{Address, B256, U256};
use alloy_sol_types::SolValue;
use celestia_types::hash::Hash;
use celestia_types::{AppVersion, MerkleProof};
use risc0_steel::config::ChainSpec;
use risc0_steel::ethereum::EthBlockHeader;
use risc0_steel::{ethereum::EthEvmInput, Commitment, Contract, EvmEnv, StateDb};
use risc0_zkvm::guest::env;
use toolkit::blobstream::{
    BinaryMerkleProof, Blobstream0,
    DataRootTuple, IDAOracle,
};
use toolkit::journal::Journal;
use toolkit::{
    share_proof_start_index_ods, BlobIndex, BlobProofData, BlobstreamAttestation,
    BlobstreamAttestationAndRowProof, DaChallengeGuestData, SpanSequence,
};
use toolkit::errors::{compute_ods_width_from_row_proof, DaFraud, DaGuestError, InputError};

risc0_zkvm::guest::entry!(main);

fn verify_blobstream_attestation(
    blobstream_contract: &Contract<&EvmEnv<StateDb, EthBlockHeader, Commitment>>,
    blobstream_attestation: &BlobstreamAttestation,
) {
    let formatted_proof = BinaryMerkleProof::from(blobstream_attestation.proof.clone());

    let blobstream_call = IDAOracle::verifyAttestationCall {
        _tupleRootNonce: U256::from(blobstream_attestation.nonce),
        _tuple: DataRootTuple {
            height: U256::from(blobstream_attestation.height),
            dataRoot: B256::from_slice(&blobstream_attestation.data_root),
        },
        _proof: formatted_proof,
    };

    // `verifyAttestation()` returns nothing, discard the return value
    let _blobstream_return = blobstream_contract.call_builder(&blobstream_call).call();
}

fn get_current_blobstream_height(
    blobstream_contract: &Contract<&EvmEnv<StateDb, EthBlockHeader, Commitment>>,
) -> u64 {
    let height_call = Blobstream0::latestHeightCall {};
    blobstream_contract.call_builder(&height_call).call()._0
}

fn verify_blobstream_attestation_and_row_proof(
    blobstream_contract: &Contract<&EvmEnv<StateDb, EthBlockHeader, Commitment>>,
    BlobstreamAttestationAndRowProof {
        blobstream_attestation,
        row_proof,
        row_root_node,
    }: &BlobstreamAttestationAndRowProof,
) {
    verify_blobstream_attestation(blobstream_contract, blobstream_attestation);

    // TODO: this serialization can be performed on the host side
    let serialized_row_root_node =
        borsh::to_vec(&row_root_node).expect("failed to serialize row root");

    row_proof
        .verify(&serialized_row_root_node, blobstream_attestation.data_root)
        .expect("failed to verify row proof");
}

fn verify_span_sequence_inclusion(
    span_sequence: &SpanSequence,
    row_proof: &MerkleProof,
) -> Result<(), DaGuestError> {
    let ods_width = compute_ods_width_from_row_proof(row_proof)?;
    let ods_size = ods_width * ods_width;

    let last_share_index = span_sequence.end_index_ods()?;
    
    env::log(&format!("last_share_index: {}", last_share_index));

    if last_share_index > ods_size {
        env::log(&format!(
            "invalid blob commitment end index: {} > {}",
            last_share_index, ods_size
        ));
        return Err(DaFraud::ShareIndexOutOfBounds {
            share_index: last_share_index,
            ods_size,
        }
        .into());
    }

    Ok(())
}

fn verify_share_proofs(
    span_sequence: &SpanSequence,
    blobstream_attestation: &BlobstreamAttestation,
    blob_proof_data: &BlobProofData,
) -> Result<(), DaGuestError> {
    let span_sequence_end = span_sequence.end_index_ods()?;
    
    for share_index in span_sequence.start..span_sequence_end {
        let share_proof = &blob_proof_data.share_proofs[&share_index];
        // Check that the share belongs to the expected Celestia block
        share_proof
            .verify(Hash::Sha256(blobstream_attestation.data_root))
            .expect("failed to verify share proof");

        // Check that the share matches the expected index
        let proof_start_index_ods = share_proof_start_index_ods(share_proof);
        assert_eq!(
            proof_start_index_ods, share_index,
            "invalid share proof start index"
        );
    }
    
    Ok(())
}

fn check_block_height_bounds(
    span_sequence: SpanSequence,
    blobstream_contract: &Contract<&EvmEnv<StateDb, EthBlockHeader, Commitment>>,
    first_blobstream_attestation: BlobstreamAttestation,
) -> Result<(), DaGuestError> {
    // Assert that the proof is for the first Blobstream event by checking the nonce.
    // Nonces start at 1 in both SP1 and RISC Zero Blobstream contracts.
    if first_blobstream_attestation.nonce != 1 {
        return Err(InputError::InvalidFirstBlobstreamAttestationNonce.into());
    }
    // Assert that the proof is for the first Celestia block to guarantee that this is truly
    // the lower bound.
    if first_blobstream_attestation.proof.index != 0 {
        return Err(InputError::InvalidFirstBlobstreamAttestationIndex.into());
    }
    verify_blobstream_attestation(blobstream_contract, &first_blobstream_attestation);

    let min_block_height = first_blobstream_attestation.height;
    if span_sequence.height < min_block_height {
        return Err(DaFraud::BlockHeightTooLow {
            block_height: span_sequence.height,
            min_block_height,
        }
        .into());
    }

    let max_block_height = get_current_blobstream_height(blobstream_contract);
    if span_sequence.height > max_block_height {
        return Err(DaFraud::BlockHeightTooLow {
            block_height: span_sequence.height,
            min_block_height,
        }
            .into());
    }

    Ok(())
}

fn check_da_challenge(
    evm_env: &EvmEnv<StateDb, EthBlockHeader, Commitment>,
    blobstream_address: Address,
    serialized_da_guest_data: Vec<u8>,
) -> Result<(), DaGuestError> {
    let DaChallengeGuestData {
        index_blob,
        challenged_blob,
        index_blob_proof_data: index_blob_data,
        block_proofs,
        first_blobstream_attestation,
    } = bincode::deserialize(&serialized_da_guest_data).expect("failed to deserialize guest data");

    let blobstream_contract = Contract::new(blobstream_address, evm_env);

    // Verify the authenticity of all the provided block proofs.
    for (block_height, block_proof) in &block_proofs {
        assert_eq!(
            *block_height, block_proof.blobstream_attestation.height,
            "invalid block height"
        );
        verify_blobstream_attestation_and_row_proof(&blobstream_contract, block_proof);
    }

    // If the index blob is the missing blob, verify exclusion immediately.
    if challenged_blob == index_blob {
        // Verify that the index blob is excluded
        check_block_height_bounds(
            index_blob,
            &blobstream_contract,
            first_blobstream_attestation,
        )?;
        return verify_span_sequence_inclusion(
            &index_blob,
            &block_proofs[&index_blob.height].row_proof,
        );
    }

    // To go any further, the index blob data must be present.
    let index_blob_data = index_blob_data.ok_or(InputError::MissingIndexBlobData)?;

    // Verify the share proofs of the index blob
    verify_share_proofs(
        &index_blob,
        &block_proofs[&index_blob.height].blobstream_attestation,
        &index_blob_data,
    )?;
    // Deserialize the index blob
    let app_version =
        AppVersion::from_u64(index_blob_data.app_version).expect("invalid app version");
    let index = BlobIndex::reconstruct_from_raw(index_blob_data.shares(), app_version)?;

    // Iterate over the blobs in the index and check if they're the missing blob.
    for blob_commitment in index.blobs {
        if challenged_blob == blob_commitment {
            check_block_height_bounds(
                challenged_blob,
                &blobstream_contract,
                first_blobstream_attestation,
            )?;
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
    let chain_spec: ChainSpec = env::read();
    let blobstream_address: Address = env::read();
    let serialized_da_guest_data: Vec<u8> = env::read_frame();

    // Converts the input into a `EvmEnv` for execution. The `with_chain_spec` method is used
    // to specify the chain configuration. It checks that the state matches the state root in the
    // header provided in the input.
    let evm_env = input.into_env().with_chain_spec(&chain_spec);

    match check_da_challenge(&evm_env, blobstream_address, serialized_da_guest_data) {
        Ok(()) => panic!("the specified blob is available, DA challenge failed"),
        Err(DaGuestError::Input(err)) => {
            panic!("invalid input: {}", err)
        }
        Err(DaGuestError::Fraud(err)) => env::log(&format!("DA challenge success: {err}")),
    }

    // Commit the block hash and number used when deriving `view_call_env` to the journal.
    let journal = Journal {
        commitment: evm_env.into_commitment(),
        blobstreamAddress: blobstream_address,
    };
    env::commit_slice(&journal.abi_encode());
}
