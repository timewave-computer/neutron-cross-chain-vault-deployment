use std::collections::HashMap;

use cosmwasm_std::{coin, Coin};
use log::warn;
use valence_clearing_queue_supervaults::state::WithdrawalObligation;

use crate::phases::SETTLEMENT_PHASE;

// TODO: deprecate this in favor of `batch_obligation_queue_payout_coins` below.
/// helper function that flattens a vec of withdraw obligations
/// into a single batch.
/// returns a tuple: (amount_1, amount_2), respecting the order
/// of the (denom_1, denom_2) input
pub fn flatten_obligation_queue_amounts(
    obligations: &[WithdrawalObligation],
    (denom_1, denom_2): (String, String),
) -> (u128, u128) {
    let mut amount_1 = 0;
    let mut amount_2 = 0;

    // iterate through all obligations and sum up the coin amounts
    for withdraw_obligation in obligations.iter() {
        for payout_coin in withdraw_obligation.payout_coins.iter() {
            if payout_coin.denom == denom_1 {
                amount_1 += payout_coin.amount.u128();
            } else if payout_coin.denom == denom_2 {
                amount_2 += payout_coin.amount.u128();
            } else {
                warn!(target: SETTLEMENT_PHASE, "obligation contains unrecognized denom: {}", payout_coin.denom);
            }
        }
    }

    (amount_1, amount_2)
}

/// batches a given vec of withdrawal obligation payouts into a vec of coins
pub fn batch_obligation_queue_payout_coins(obligations: &[WithdrawalObligation]) -> Vec<Coin> {
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
