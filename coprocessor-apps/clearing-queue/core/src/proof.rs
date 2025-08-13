use alloc::vec::Vec;
use alloy_primitives::U256;
use valence_coprocessor::DomainCircuit;
use valence_coprocessor::StateProof;
use valence_coprocessor_ethereum::{Ethereum, EthereumProvenAccount, EthereumStorageProofArg};

use crate::VAULT_ADDRESS;
use crate::WithdrawRequest;

pub fn verify_proof(
    proof: &StateProof,
    withdraw: &WithdrawRequest,
) -> anyhow::Result<Vec<EthereumStorageProofArg>> {
    let EthereumProvenAccount {
        account, storage, ..
    } = Ethereum::verify(proof)?;

    anyhow::ensure!(hex::encode(account) == &VAULT_ADDRESS[2..]); // strips "0x"
    let value = [&withdraw.owner.into_array()[..], &withdraw.id.to_be_bytes()].concat();
    anyhow::ensure!(Some(rlp::encode(&value).to_vec()) == storage[0].value);

    let value = withdraw.redemptionRate.to_be_bytes_trimmed_vec();
    anyhow::ensure!(Some(rlp::encode(&value).to_vec()) == storage[1].value);

    let value = withdraw.sharesAmount.to_be_bytes_trimmed_vec();
    anyhow::ensure!(Some(rlp::encode(&value).to_vec()) == storage[2].value);

    let receiver_len = withdraw.receiver.len() as u64;
    if receiver_len <= 31 {
        // Short string: packed with length in the same slot
        let mut value = withdraw.receiver.as_bytes().to_vec();
        value.resize(32, 0); // Pad to 32 bytes
        // For short strings, the length is stored in the last byte (LSB)
        value[31] = (receiver_len << 1) as u8; // Set length with the encoding bit

        anyhow::ensure!(Some(rlp::encode(&value).to_vec()) == storage[3].value);
    } else {
        // Long string: length in slot 3, data in subsequent slots
        let value = (receiver_len << 1) + 1;
        let value = U256::from(value).to_be_bytes_trimmed_vec();
        anyhow::ensure!(Some(rlp::encode(&value).to_vec()) == storage[3].value);

        // Data chunks in subsequent slots
        for (i, c) in withdraw.receiver.as_bytes().chunks(32).enumerate() {
            let mut value = c.to_vec();
            value.resize(32, 0);
            anyhow::ensure!(Some(rlp::encode(&value).to_vec()) == storage[4 + i].value);
        }
    }

    Ok(storage)
}
