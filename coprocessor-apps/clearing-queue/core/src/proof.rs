use alloc::vec::Vec;
use alloy_primitives::{FixedBytes, U256};
use alloy_rlp::Encodable as _;
use alloy_rpc_types_eth::{Account, EIP1186AccountProofResponse};
use alloy_serde::JsonStorageKey;
use alloy_trie::Nibbles;

use crate::{VAULT_ADDRESS_HASH, WithdrawRequest};

pub fn verify_proof(
    proof: &EIP1186AccountProofResponse,
    withdraw: &WithdrawRequest,
    state_root: &[u8],
) -> anyhow::Result<()> {
    let state_root: FixedBytes<32> = TryFrom::try_from(state_root)?;
    let key = Nibbles::unpack(VAULT_ADDRESS_HASH);

    let mut account = Vec::new();
    Account {
        nonce: proof.nonce,
        balance: proof.balance,
        storage_root: proof.storage_hash,
        code_hash: proof.code_hash,
    }
    .encode(&mut account);

    alloy_trie::proof::verify_proof(state_root, key, Some(account), &proof.account_proof)
        .map_err(|e| anyhow::anyhow!("account proof failed: {e}"))?;

    let root = proof.storage_hash;
    let mut proof = proof.storage_proof.iter();

    // id+owner slot

    let p = proof
        .next()
        .ok_or_else(|| anyhow::anyhow!("id/owner not available"))?;
    let key = match p.key {
        JsonStorageKey::Hash(b) => b,
        JsonStorageKey::Number(_) => anyhow::bail!("invalid storage proof"),
    };
    let key = alloy_primitives::keccak256(key);
    let key = Nibbles::unpack(key);
    let value = [&withdraw.owner.into_array()[..], &withdraw.id.to_be_bytes()].concat();
    let value = rlp::encode(&value).to_vec();
    alloy_trie::proof::verify_proof(root, key, Some(value), &p.proof)
        .map_err(|e| anyhow::anyhow!("storage proof failed: {e}"))?;

    // redemption rate

    let p = proof
        .next()
        .ok_or_else(|| anyhow::anyhow!("redemption rate not available"))?;
    let key = match p.key {
        JsonStorageKey::Hash(b) => b,
        JsonStorageKey::Number(_) => anyhow::bail!("invalid storage proof"),
    };
    let key = alloy_primitives::keccak256(key);
    let key = Nibbles::unpack(key);
    let value = withdraw.redemptionRate.to_be_bytes_trimmed_vec();
    let value = rlp::encode(&value).to_vec();
    alloy_trie::proof::verify_proof(root, key, Some(value), &p.proof)
        .map_err(|e| anyhow::anyhow!("storage proof failed: {e}"))?;

    // shares amount

    let p = proof
        .next()
        .ok_or_else(|| anyhow::anyhow!("shares amount not available"))?;
    let key = match p.key {
        JsonStorageKey::Hash(b) => b,
        JsonStorageKey::Number(_) => anyhow::bail!("invalid storage proof"),
    };
    let key = alloy_primitives::keccak256(key);
    let key = Nibbles::unpack(key);
    let value = withdraw.sharesAmount.to_be_bytes_trimmed_vec();
    let value = rlp::encode(&value).to_vec();
    alloy_trie::proof::verify_proof(root, key, Some(value), &p.proof)
        .map_err(|e| anyhow::anyhow!("storage proof failed: {e}"))?;

    // receiver

    let mut len = withdraw.receiver.len();
    let mut receiver = withdraw.receiver.as_str();
    let p = proof
        .next()
        .ok_or_else(|| anyhow::anyhow!("receiver initial slot not available"))?;

    if len < 32 {
        let key = match p.key {
            JsonStorageKey::Hash(b) => b,
            JsonStorageKey::Number(_) => anyhow::bail!("invalid storage proof"),
        };
        let key = alloy_primitives::keccak256(key);
        let key = Nibbles::unpack(key);

        // Create 32-byte slot with string data and length encoding
        let mut slot_value = [0u8; 32];
        let receiver_bytes = withdraw.receiver.as_bytes();

        // Copy string data to the left side of the slot
        slot_value[..receiver_bytes.len()].copy_from_slice(receiver_bytes);

        // Encode length in the rightmost byte (length * 2)
        slot_value[31] = (len * 2) as u8;

        let value = rlp::encode(&slot_value.to_vec()).to_vec();
        alloy_trie::proof::verify_proof(root, key, Some(value), &p.proof)
            .map_err(|e| anyhow::anyhow!("storage proof failed: {e}"))?;
    } else {
        let key = match p.key {
            JsonStorageKey::Hash(b) => b,
            JsonStorageKey::Number(_) => anyhow::bail!("invalid storage proof"),
        };
        let key = alloy_primitives::keccak256(key);
        let key = Nibbles::unpack(key);
        let value = (len << 1) + 1;
        let value = U256::from(value as u64).to_be_bytes_trimmed_vec();
        let value = rlp::encode(&value).to_vec();
        alloy_trie::proof::verify_proof(root, key, Some(value), &p.proof)
            .map_err(|e| anyhow::anyhow!("storage proof failed: {e}"))?;

        while len > 0 {
            let value = len.saturating_sub(32);
            let value = len - value;
            let (curr, new) = receiver.split_at(value);

            let p = proof
                .next()
                .ok_or_else(|| anyhow::anyhow!("receiver contents not available"))?;
            let key = match p.key {
                JsonStorageKey::Hash(b) => b,
                JsonStorageKey::Number(_) => anyhow::bail!("invalid storage proof"),
            };
            let key = alloy_primitives::keccak256(key);
            let key = Nibbles::unpack(key);

            let mut value = curr.as_bytes().to_vec();
            value.resize(32, 0);

            let value = rlp::encode(&value).to_vec();

            alloy_trie::proof::verify_proof(root, key, Some(value), &p.proof)
                .map_err(|e| anyhow::anyhow!("storage proof failed: {e}"))?;

            receiver = new;
            len = len.saturating_sub(32);
        }
    }

    Ok(())
}
