use std::collections::HashMap;

use log::warn;
use valence_clearing_queue_supervaults::state::WithdrawalObligation;

use crate::phases::SETTLEMENT_PHASE;

// TODO: remove this and use the batch function below
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

pub fn batch_obligation_queue_payouts(
    obligations: &[WithdrawalObligation],
) -> HashMap<String, u128> {
    let mut amount_map: HashMap<String, u128> = HashMap::new();

    for obligation in obligations {
        for payout_coin in &obligation.payout_coins {
            *amount_map.entry(payout_coin.denom.to_string()).or_insert(0) +=
                payout_coin.amount.u128();
        }
    }

    amount_map
}
