use alloy_rlp::{RlpDecodable, RlpEncodable};
use alloy_sol_types::SolValue as _;
use serde::{Deserialize, Serialize};

alloy_sol_types::sol! {
    #![sol(extra_derives(Debug, Serialize, Deserialize, RlpEncodable, RlpDecodable))]
    struct WithdrawRequest {
        uint64 id;
        address owner;
        uint256 redemptionRate;
        uint256 sharesAmount;
        string receiver;
    }
}

impl WithdrawRequest {
    pub fn try_from_eth_call(result: &[u8]) -> anyhow::Result<Self> {
        // ABI decode requires a prefix with an offset for the data, that is not available on a
        // eth_call. We skip the prefix by setting an offset of 0x20
        const PREFIX: &[u8] = &[
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0x20,
        ];

        let data = [PREFIX, result].concat();
        let withdraw = WithdrawRequest::abi_decode(&data, true)?;

        Ok(withdraw)
    }
}

#[test]
fn eth_call_to_withdraw_works() {
    // contract 0xf2b85c389a771035a9bd147d4bf87987a7f9cf98
    // withdraw request `1`
    // curl -X POST https://eth-mainnet.g.alchemy.com/v2/$ALCHEMY
    //  -H "Content-Type: application/json"   -d '{
    //     "jsonrpc":"2.0",
    //     "method":"eth_call",
    //     "params":[
    //        {
    //          "to": "0xf2B85C389A771035a9Bd147D4BF87987A7F9cf98",
    //          "data": "0x94ba2b8d000000000000000000000000000000000000000000000000000000000
    // 0000001"
    //        },
    //        "0x15C274A"
    //     ],
    //     "id":1
    //   }'

    let data = "0000000000000000000000000000000000000000000000000000000000000001000000000000000000000000d9a23b58e684b985f661ce7005aa8e10630150c10000000000000000000000000000000000000000000000000000000005f5e10000000000000000000000000000000000000000000000000000000000000000c800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000000426e657574726f6e316d32656d6339336d3967707767737273663276796c76397876677168363534363330763764667268726b6d7235736c6c79353373706738357776000000000000000000000000000000000000000000000000000000000000";
    let data = hex::decode(data).unwrap();
    let withdraw = WithdrawRequest::try_from_eth_call(data.as_slice()).unwrap();

    assert_eq!(withdraw.id, 1);
    assert_eq!(
        hex::encode(withdraw.owner),
        "d9a23b58e684b985f661ce7005aa8e10630150c1"
    );
    assert_eq!(withdraw.redemptionRate.to::<u64>(), 100000000);
    assert_eq!(withdraw.sharesAmount.to::<u64>(), 200);
    assert_eq!(
        withdraw.receiver,
        "neutron1m2emc93m9gpwgsrsf2vylv9xvgqh654630v7dfrhrkmr5slly53spg85wv"
    );
}
