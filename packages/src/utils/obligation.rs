use std::collections::HashMap;

use cosmwasm_std::{coin, Coin};
use valence_clearing_queue_supervaults::state::WithdrawalObligation;

/// batches a given vec of withdrawal obligation payouts into a vec of coins
pub fn batch_obligation_queue_payouts(obligations: &[WithdrawalObligation]) -> Vec<Coin> {
    let mut totals: HashMap<String, u128> = HashMap::new();

    for ob in obligations {
        for coin in &ob.payout_coins {
            totals
                .entry(coin.denom.to_string())
                .and_modify(|amt| *amt += coin.amount.u128())
                .or_insert(coin.amount.u128());
        }
    }

    totals
        .into_iter()
        .map(|(denom, amount)| coin(amount, denom))
        .collect()
}
