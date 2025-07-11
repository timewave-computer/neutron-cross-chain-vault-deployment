#![no_std]
extern crate alloc;

use alloc::{format, string::ToString as _, vec, vec::Vec};
use alloy_primitives::U256;
use alloy_rpc_types_eth::EIP1186AccountProofResponse;
use alloy_sol_types::SolValue;
use clearing_queue_core::{VAULT_ADDRESS, WithdrawRequest};
use serde_json::{Value, json};
use valence_coprocessor::{StateProof, Witness};
use valence_coprocessor_wasm::abi;

const NETWORK: &str = "eth-mainnet";
const DOMAIN: &str = "ethereum-alpha";

/// Fn selector defined as Keccak256("withdrawRequests(uint64)")[..4]
const FN_SELECTOR: &[u8] = &[0x94, 0xba, 0x2b, 0x8d];

/// slot value of the storageLayout of the ABI. Can be obtained via foundry.
const WITHDRAWS_MAPPING_SLOT: u64 = 0xA;

pub fn get_witnesses(args: Value) -> anyhow::Result<Vec<Witness>> {
    let block =
        abi::get_latest_block(DOMAIN)?.ok_or_else(|| anyhow::anyhow!("no valid domain block"))?;

    let root = block.root;
    let block = format!("0x{:x}", block.number);

    let withdraw_request_id = args["withdraw_request_id"].as_u64().unwrap();

    let encoded = (withdraw_request_id).abi_encode();
    let encoded = [FN_SELECTOR, &encoded].concat();
    let encoded = hex::encode(encoded);
    let encoded = ["0x", encoded.as_str()].concat();

    let data = abi::alchemy(
        NETWORK,
        "eth_call",
        &json!(
            [
                {
                    "to": VAULT_ADDRESS,
                    "data": encoded,
                },
                block,
            ]
        ),
    )?;

    let data = data
        .as_str()
        .and_then(|d| d.strip_prefix("0x"))
        .ok_or_else(|| anyhow::anyhow!("invalid response"))?;

    let data = hex::decode(data).map_err(|e| anyhow::anyhow!("invalid response hex: {e}"))?;
    let withdraw = WithdrawRequest::try_from_eth_call(&data)?;

    let id = U256::from(withdraw_request_id).to_be_bytes::<32>();
    let slot = U256::from(WITHDRAWS_MAPPING_SLOT).to_be_bytes::<32>();
    let slot = [id, slot].concat();
    let slot = alloy_primitives::keccak256(slot);

    let slot_id_owner = U256::from_be_slice(slot.as_slice());
    let slot_redemption_rate = slot_id_owner + U256::from(1);
    let slot_shares_amount = slot_id_owner + U256::from(2);
    let slot_receiver_pointer = slot_id_owner + U256::from(3);

    let mut storage_keys_to_prove = vec![
        slot_id_owner,
        slot_redemption_rate,
        slot_shares_amount,
        slot_receiver_pointer,
    ];

    let receiver_slots = if withdraw.receiver.len() < 32 {
        0
    } else {
        withdraw.receiver.len().div_ceil(32)
    } as u64;

    let slot = alloy_primitives::keccak256(slot_receiver_pointer.to_be_bytes::<32>());
    let slot = U256::from_be_slice(slot.as_slice());
    for i in 0..receiver_slots {
        storage_keys_to_prove.push(slot + U256::from(i));
    }

    let proof = abi::alchemy(
        NETWORK,
        "eth_getProof",
        &json!([VAULT_ADDRESS, storage_keys_to_prove, block]),
    )?;

    let proof: EIP1186AccountProofResponse = serde_json::from_value(proof)?;

    abi::log!(
        "{}",
        serde_json::to_string(&json!({
            "withdraw": withdraw,
            "account": VAULT_ADDRESS,
            "proof": proof,
            "block": block,
            "root": format!("0x{}", hex::encode(root)),
        }))
        .unwrap_or_default()
    )?;

    let proof = bincode::serde::encode_to_vec(proof, bincode::config::standard())?;
    let withdraw = bincode::serde::encode_to_vec(withdraw, bincode::config::standard())?;

    let proof = Witness::StateProof(StateProof {
        domain: DOMAIN.into(),
        root,
        payload: Default::default(),
        proof,
    });
    let withdraw = Witness::Data(withdraw);

    Ok(vec![proof, withdraw])
}

pub fn entrypoint(args: Value) -> anyhow::Result<Value> {
    abi::log!(
        "received an entrypoint request with arguments {}",
        serde_json::to_string(&args).unwrap_or_default()
    )?;

    let cmd = args["payload"]["cmd"].as_str().unwrap();

    match cmd {
        "store" => {
            let path = args["payload"]["path"].as_str().unwrap().to_string();
            let bytes = serde_json::to_vec(&args).unwrap();

            abi::set_storage_file(&path, &bytes).unwrap();
        }

        _ => panic!("unknown entrypoint command"),
    }

    Ok(args)
}

#[test]
fn verification_long_string_works() {
    use alloy_rpc_types_eth::EIP1186AccountProofResponse;
    use serde_json::Value;

    use crate::WithdrawRequest;

    /*
    How to get this:
    curl -X POST http://prover.timewave.computer:37281/api/registry/controller/a38836c50ecf341f3ece09c1723df4cca66a95723534956ba709c6e9f2d6ad5b/witnesses -H "Content-Type: application/json" -d '{"withdraw_request_id": 0}' | jq '.log[0]' | jq -r
     */
    let data = r#"{
        "account": "0x9fe5b9c7ddbd26d0dc93634e15eb1a5d34c85493",
        "block": "0x15d5cce",
        "proof": {
            "accountProof": [
                "0xf90211a0c6a71f674b56f26ae3418ad67ae0f0c0a7d8b4de1472e70d3a4beb04d6b31381a08956159ef5f497b63ce1ecd741a190218a1f4014677335b288d6ecbb4fb5fcb0a00b6e66cc10a09a9feaf28d43ed2b2ab66d1aa208943c1c91abea937a66fe2ac2a00c0b111ff5c6647a16b15261f1fb6a2495c9f7ad8fb979f6c09fd36599d215a4a0b7ca212650ceb387b5149cb77e0355ccded7054dd1c397b2c3efa7c2501d51cea0ee2c7125f22e3d3606c0abf32f0f4eeb3a8813636e98f9e1aa48acc7ef02259ea04e2b3c3199d64ea0438191fe16481e914c009b6a5c49b0be2394e0f55c08d287a0f3c0c577420c0c86211e8df5998df5d30ef033c1cc6d6d5a433555e8c8480c5fa0e903e2fbbeadae819b65956379bcf93ee279dc6b799db69c3de1ff7fca1e29e2a045b4a20f8d0c29a47b019ab4545dc017a04a4513bea084d8d086fe2cf1df588ca014b29302940b2b07a05c0e9ebe62ef4215cbc92022ddf04028fbbde15995139aa009fe8579082535a79507e1843f88ccfce7679a96d377a9642c1628b9b99d2b4ba012b9cc16ce4e520710099c28daeda2ebe4147745478f1a078065a84086535946a04a9a2c4ee25cfaf1c4430c28fbf82dd40233b147c0cd3761aaa028a18519822ca0586f1d0e142c5a203a807f62615aee3d9c825a68c2a10387a533b3f70ddd7bffa0ba2a5f9823dd7643dbb1244b910d7a3ebfe3b4c498d8e6aa47f23f4f161a801880",
                "0xf90211a0489e52334d510594b5b77c2017a7e6f8709cba3d139f596e15212e15f2e018f5a071346ac678315ee6f466c1c86d5b8662d874b1c1e0964067a4b4099f7abda8d5a0160325aecbde439c95fbb9a2e75376ef7666aa8f4a5dd5865431b60247eb3bdca0233f092312380b7ef55ebd2dab361ce5e30548b2fbfb8446ae2e6a01d2d320afa003f9b939d143df9f7dda073c2bb536687583581b702476d6e0b62c0dda2bbdf5a0789fe13d5459fbed673b39de7c26ac549de9624235ed9f41c8f8fb3e04efd194a0c3faf906bf53949e5a4e76066422dfea48f2dce579dc41ff16c3eab6e8349151a0cd379a441c44372ee122d26703fd909a5429f643dd7c36d3b630dfb7226651fba03e1d622454dfbe521c385e3a588a9b810dab4f92cfc405dcdd65abcdef9e38c8a09fbc77df08ceb4f04350b8f5637ec11d5954a7ec603e9e523a4c665eb0edc253a015ae167c3eea8b920cc0d0c30267bf1df54580eb592809f06f2afeebf033490fa0d16bc17dafa03f2373009a522cbcb802cdf308f7ca878ea861c9133509d275dda013e38732ca68407589f595e0114bbfd429d8f7581a085eeaae6eb9e622e8a3a5a0da5c5df9cf42a75aa96329b281e661541d0bd8387fd06161532c7710bbc72764a0911549c10abb06344c72d622c816621285e37421e8b0911d15975de47e879f5aa02f74f8f77ec4db93619354823f1607579e6513828117b32f58fbdfdeb3757e3580",
                "0xf90211a0941b348bef03f2c3946489caa013d4815a78cd6811f683013d32fb12158d533da0a78c4c85992510b4a86e08edc9c91b6f077b76d37da70f571e6e1b6829900896a0a790ee43550d0f44643c9f36da4ffc25ef5b8f557b8a4275e59adf39099bcbf0a0b35428d07e9ef51346f1a91762ded5b3af5d701fdd07d7cf6eeb55cdfe6ef110a0b9cafdf3e0bd9380ad251c3699ce7b66ea170462ac8093abfe5d0970fdaaa293a0bc2f16301f4e07bf3256ae2e850640d7f569cb87597cd09e28f00cfe07abd3b7a0f62a0e474c60956977e06408eeeb87e05797d6fb880c406eba3220aa70f2f68fa029b57e0d18943675b384d9f4dca9ec3f689dd9258d781434ebc6c12dffeafb31a0cdb40ad6e7eb24c0cf1a7b00a7feb2442a43eb0113cc4917c1d11a90e9cf93caa04387c105577924549f6b06f735b291b95a113ed3455f75d414c89912676c97c9a0bcea7d8ad4ea2db092a46ea26b3b94332ed78c446be9577f04ca50b2531675bba0bca3b4ab72947a4c923e687d3ac3631239da682f02144dece14ce355c5cd1306a0610f58e7b0a55c5a6a49b137a40ccaa87496e7bbcea7b5f538cbc4f659c58ddba006144ac659be5c4df416b05068a18df3b861e423bf2b0b31788291d8aa4bef4ca0083a9fe338569b7d5d19b18ae69b06bcc4a082d6324a71baa86e4f852d615f3fa00ae3a089cefdb307ab3626c2c8322386db4c0b51c4ebd7e3e5561e7f5ca1698780",
                "0xf90211a05f5fc3c4e6568b3a7cfac75fa6f51abd8d2da412c9b3c9ac8d530230507192b7a05fc6ce57ebc5baca189d76f10d44156d2462b59eefe9226e0e1ea1f02da493b1a09e90572c54eb6ceed3cf8d634dddfc0fb5a94179780407d408973fe455c788daa0a4cf1d0c55a226424bb4142c1495e41d30c24e608cd0d64591c1efe9f34a616ea0818f162807c3a9112482d03f8fc445f89457c6585c689679c13d08b21d8fe775a0bfb7df6a4a3bd80ed7c847ddfc95d93df857327aa33a116525e70f92df56fcc4a0cbc8b7c04e81e81c705d33e5a504af07dfdacecf225ea1eca862c73c6482299ba00a03c09f099433c055372f4016b194a7687e64068a0a602fb8e4ed4437b44617a06d81c8b6ea8cab31e7a86c1779539c1ebf5b6b9b5aa9b7fc66b46da56a138de7a0f7656bf841b4add8f2b5e7c6571bf6c560a3d6eb85c3d0d666bdcfdcea039321a0630738bd0bff25be60785fe2e5c762fbcc67d553a33c6bbed8d8811c5f0baf8da0f35f536a4f18c20088244f0892a756c4ef3ec02cf3e32309488f06c4c08bb926a078a5e3bd7e7ed2999e36584d643fda679914dc1031f5be4c41bd429622354c5aa0aa384e61561e0643eac1ad70e254918fcc95a81b0c3ee9839c46ca3261d2d0aca024f2b31c434fd506409b076a4d808ac6da885327eb4d82a8be9ff6a652c0d8eda072903d40154f85863875ba59035dd62e113ed38e81ff083cf92cc332218dd3e580",
                "0xf90211a0d97089aaa0d5ef450cfdb43b9d76fab4fa70d34bc6d939833815898054ffd691a0e9b15af928bd46686c6115c328785231a9146a357f1acf42900779711774b55ea02d6ca6d93a6c5c1e45d21fe0e3aa086da968b8ad274b800fd0f76405671cc762a06de21d616b241302953dbea9ff8e363153c7f8208f2b47ad22c651954705b84fa0aadf39a5d6e9fadc01d6da1838ecb92b368ab993a2787e7e59325662e13dc35aa00448239a5e29c46403263f90ea8e88e64be9eda5cbf9ac54965ea0caf3429171a03c1c6be6c47dd1266b35d354e29da3783e70d1f7141d629fadeeb9ab5320e443a0d585b87369120e0dd22be0f9c6997b9f0ac21dd7d730ad2d0d08f6045ea14963a0f42b8fb2e64fee1649761e38508a715b7618009e88ff230fed8e5cf7d362af2ca06947edf952b1b2c7caf5fa11d3add1eaedeaed58c0af0bf7b3474af2c430d73ba015fe673b29860fe9afa9035a87d9eecc34b3e43d0e95fc880ec68ba765128597a0e2fffe68f9a3588a82d75db057ce5d3fdd8aa8b8f3df28c1a7d929df971e0028a0201885aaf4cfc17fa1d13c8a38f3324db8a8055e9bb2e7e05568c4d0ea7082d8a0d03e04d9f94ce8a40593510077daa3dc7ae0970263507cd645facce9f1cf8d76a03d6cb695c9336c0625b9704c236f654420d6cf416fe773aeb192e0dc97262e8fa02ccddb9cb86ca8893a0a7e1c3a0a8edd1d01ed9712963b84ca8326c822d4f4a780",
                "0xf90211a01b5b39ee676321d746eec5861ee020cf174aa078c394af882bd77e8fb57c098ba0ebd7ae53deca65f0cb63664994111f22d5e700f0165b32d00cb99a8b4afe5367a092aa32b62af092f8b644ef7dcdb675380b7517f975fedd917538e607e9d030e0a0846e6dbb70b4c2cf8a8beca3de59a9a584b29ad23087b6f51f67db62687e165ba003e5c0bbcf2cb0ce8c8f6edf581b368008ed0f444132b02554bebc3b4ced3c28a0839a0e4491332a6bb87824ebb3ffedf20f46f5bf5c885c02702b69ab7154de78a053c7607ca0eede1c0e96a5cee03934575e538ba4c261c143bd3ee5b5d0a3bde8a0cea6ddb3693d6f738a6109e255cf866829c650b3be0136569c840272039ae679a0837dd4b135b90988165dcc55f4999cdd21feb781a8adf3c481b1da68d83b3feba012da2efa5d88b9afebca22231a07fb54e819363b14f279ba47b7d91ad633e984a080b1ccda0b196e83df16fc567f85f49f254cd78ba8528fb3d7505e60ab5e01e0a047ada3d5a68c7b3d9c5408e88213eb874cfef3eeb7bf9c7719dcd853d78d70cda09ea9969a43951f4aa9f807ac142f008f0326250df85e0108b845f14f08dfbc6ba09e23beb005993211d3ff2045331e27f9ec95c3efa0459ae3d4a445f581350dffa01a70614f3f0b5134eb1062654a378acebb9c12e241c0ac6c0fdf7cb76dc160f8a086b14f191a93845b3e3f6d13615c29ffc5a6a4beffe6d0ae241225416e8e2b2180",
                "0xf9019180a0b991ff928edfaf86c57439c18435610d32de451ee3fe439670cbe4dfb9fef13fa038931af30f182aab9278b45bd959716db47be99859d863aec7d172599642ee6580a068eae662efef3f9e7a6861fa4826007876ba3e0e34143227777022f5750ddb73a08e76fe31a672b144e290a3542779ab7083179d45e3f07c5a3fbb1a0aba1fae17a07622e7f003e733737894b4f36489103caa80a4e6cd04edb05078ef6b4c5db321a0f3be4091720b3a423a66865eb9c3acd1c549d33584dda237524c7365d623794d80a065220ca9cd302b62b4fe1777a9956bc9ffb9b6d95a420a91c257c6115f4aec88a0dc79194bca73fca9d7b0ef19a91e354894f98ef62d7163ed2c9bdb024949a84da065edaa30692af7484a9495794596ac975b1db5b56faec8d868b2855ffec920eaa0c11844106d15c500498555742833219e064ce3c5f6fdd3236c903244c4ebceafa07e5da078f15e1a6e3a5766436daf4ca00cde77a6950580cb64ace9b97ca840c380a02fbee9f80b94cff6b2af4d9b07e989650479bac49a873705bf5f1f6eb341394880",
                "0xf8518080808080808080808080a0845c3b94f85446bcb7c4643ff4762cdbfb4c3d096bd090b9517f5b3e106580e9a0c0defad6e00f21c9c788c5557ad08abc396473f072eb343b447446241df09cd180808080",
                "0xf8669d2079ddeecc69b3de64ae959228111cb9530e0580237ab72b8efb35b918b846f8440180a07cc61e56ad8420e83b5c5da2eb417e64eee9c9e64ed016527864dee6f3e54248a04091afab2ffb5bb65bfad48f6fa23d9ccc0481cca8692102df677482093d7169"
            ],
            "address": "0x9fe5b9c7ddbd26d0dc93634e15eb1a5d34c85493",
            "balance": "0x0",
            "codeHash": "0x4091afab2ffb5bb65bfad48f6fa23d9ccc0481cca8692102df677482093d7169",
            "nonce": "0x1",
            "storageHash": "0x7cc61e56ad8420e83b5c5da2eb417e64eee9c9e64ed016527864dee6f3e54248",
            "storageProof": [
                {
                    "key": "0x13da86008ba1c6922daee3e07db95305ef49ebced9f5467a0b8613fcc6b343e3",
                    "proof": [
                        "0xf901d1a027903cc673323b319a5002734b747d98593df12d0e679616c23da9ee72dcd55280a06c306b7f0c26dd4592a3014d420575b0214e6f087d92093a3fcb639c00f0371fa0b395abe21b831935c13bfe163c01049b7ce15e1d68da00b62f65db626dc862ada0a0358c91d299929c41e64e0c79dc86293f63ff7d915a716474abd6dfb6a3baa7a0c4c5a4b95e6b0ca4f1112afc3bb9d067991a592a16bbf2aa234bd4c541b32420a01b4cdc62ee646cf3ff6b6a7b93e228d493c12ef47de828b21a9ecc503e96d319a0d38812229e6822be6d8997e2ce7e85685ebbde5998550650fb0439fc5eb5c66ba029e03eec53281466d7469acdde75446e96c9c4b2e2ebc493b7300ff4e8c2622080a0b0420d3e93e631647fbd24474769f4e03003d176a61f3d4012e5994062a17f3ea0e5238ae4400e927256247b79ad5999a1e6d49f69b318d9f179ba962285ddf3efa00e0f68936d608c0ef4c831ebd0635e5bf20c57cc79b6eea4083ecd21eeae3e6ea0165a0afc88dfd3a636d568bae74bd2cff21bf2f74c9e136bc6211f0c32e4e03ba0c7359fdf758df1a47c6027fe7450fc8daf80bdb3885c4219377ddab6a46d2364a02e63bdd28fe3c0e9a52970ba210698f43dd65b3f54523e8377e98b60ebbc79e380",
                        "0xf8918080a057d17cc3b6a9e09c9facac5b1fd193b8fdcdd79e87a75695b4efc308f85b844e8080a070746e202180578818d03f2185336a3a1902a1b48b07d6e51a2100444c348c97808080a04ebe95b214c4b4e2504ca7150d7ac32341cc15035a45eb3de40743ce51f2f898808080a0e82cfc376f971743aa2cfcfe1b3e878848389b97c74327d1cb0450074a5f4bc9808080",
                        "0xf83fa0203a0b972c470f18f842c35c712f9b9beaac1e8e838c1ba491d0d478900e81ca9d9c510c4a1d637ff374399826f421003b775dc3e8dc0000000000000000"
                    ],
                    "value": "0x510c4a1d637ff374399826f421003b775dc3e8dc0000000000000000"
                },
                {
                    "key": "0x13da86008ba1c6922daee3e07db95305ef49ebced9f5467a0b8613fcc6b343e4",
                    "proof": [
                        "0xf901d1a027903cc673323b319a5002734b747d98593df12d0e679616c23da9ee72dcd55280a06c306b7f0c26dd4592a3014d420575b0214e6f087d92093a3fcb639c00f0371fa0b395abe21b831935c13bfe163c01049b7ce15e1d68da00b62f65db626dc862ada0a0358c91d299929c41e64e0c79dc86293f63ff7d915a716474abd6dfb6a3baa7a0c4c5a4b95e6b0ca4f1112afc3bb9d067991a592a16bbf2aa234bd4c541b32420a01b4cdc62ee646cf3ff6b6a7b93e228d493c12ef47de828b21a9ecc503e96d319a0d38812229e6822be6d8997e2ce7e85685ebbde5998550650fb0439fc5eb5c66ba029e03eec53281466d7469acdde75446e96c9c4b2e2ebc493b7300ff4e8c2622080a0b0420d3e93e631647fbd24474769f4e03003d176a61f3d4012e5994062a17f3ea0e5238ae4400e927256247b79ad5999a1e6d49f69b318d9f179ba962285ddf3efa00e0f68936d608c0ef4c831ebd0635e5bf20c57cc79b6eea4083ecd21eeae3e6ea0165a0afc88dfd3a636d568bae74bd2cff21bf2f74c9e136bc6211f0c32e4e03ba0c7359fdf758df1a47c6027fe7450fc8daf80bdb3885c4219377ddab6a46d2364a02e63bdd28fe3c0e9a52970ba210698f43dd65b3f54523e8377e98b60ebbc79e380",
                        "0xf891a08ce402ebf4d6f9876083f7647b3c1a493e2c190e7a363c8285e364fe7ed0471c808080808080a07a0842085a92339e07c4f3d2ff3b7077c0ff6ce4734fbfa1c05eaead6205af4280a0a4d376e8a0398b515e452826f1e3d634fe73f7ebbb90104f17185930250235daa01862d45fdf8e1fe195497414025a3fbb599e45b07baae4594ec52ba341f258ca808080808080",
                        "0xe7a0202059072c3780c0a828a160ab8d83723bf48c12015bec43b138f97107fdbc488584049e88a0"
                    ],
                    "value": "0x49e88a0"
                },
                {
                    "key": "0x13da86008ba1c6922daee3e07db95305ef49ebced9f5467a0b8613fcc6b343e5",
                    "proof": [
                        "0xf901d1a027903cc673323b319a5002734b747d98593df12d0e679616c23da9ee72dcd55280a06c306b7f0c26dd4592a3014d420575b0214e6f087d92093a3fcb639c00f0371fa0b395abe21b831935c13bfe163c01049b7ce15e1d68da00b62f65db626dc862ada0a0358c91d299929c41e64e0c79dc86293f63ff7d915a716474abd6dfb6a3baa7a0c4c5a4b95e6b0ca4f1112afc3bb9d067991a592a16bbf2aa234bd4c541b32420a01b4cdc62ee646cf3ff6b6a7b93e228d493c12ef47de828b21a9ecc503e96d319a0d38812229e6822be6d8997e2ce7e85685ebbde5998550650fb0439fc5eb5c66ba029e03eec53281466d7469acdde75446e96c9c4b2e2ebc493b7300ff4e8c2622080a0b0420d3e93e631647fbd24474769f4e03003d176a61f3d4012e5994062a17f3ea0e5238ae4400e927256247b79ad5999a1e6d49f69b318d9f179ba962285ddf3efa00e0f68936d608c0ef4c831ebd0635e5bf20c57cc79b6eea4083ecd21eeae3e6ea0165a0afc88dfd3a636d568bae74bd2cff21bf2f74c9e136bc6211f0c32e4e03ba0c7359fdf758df1a47c6027fe7450fc8daf80bdb3885c4219377ddab6a46d2364a02e63bdd28fe3c0e9a52970ba210698f43dd65b3f54523e8377e98b60ebbc79e380",
                        "0xf8718080a05e7546db3694ea1694d2a401154ab5196dd7f74de4a6fa5c7bac043d9ec4350da068a4e97d2e458599c669c76b55a1ba282b33ced3afbeb387d3c313dfca817865808080808080a07bc8b0ba719ce964b67fa59ef79567b40ca8041b821f7b8c3fac7eecc39ff7f4808080808080",
                        "0xe5a0203cd70669dd7092ae1493528fe27f2a29c570eb58661e533ae5142c276acaea838226ab"
                    ],
                    "value": "0x26ab"
                },
                {
                    "key": "0x13da86008ba1c6922daee3e07db95305ef49ebced9f5467a0b8613fcc6b343e6",
                    "proof": [
                        "0xf901d1a027903cc673323b319a5002734b747d98593df12d0e679616c23da9ee72dcd55280a06c306b7f0c26dd4592a3014d420575b0214e6f087d92093a3fcb639c00f0371fa0b395abe21b831935c13bfe163c01049b7ce15e1d68da00b62f65db626dc862ada0a0358c91d299929c41e64e0c79dc86293f63ff7d915a716474abd6dfb6a3baa7a0c4c5a4b95e6b0ca4f1112afc3bb9d067991a592a16bbf2aa234bd4c541b32420a01b4cdc62ee646cf3ff6b6a7b93e228d493c12ef47de828b21a9ecc503e96d319a0d38812229e6822be6d8997e2ce7e85685ebbde5998550650fb0439fc5eb5c66ba029e03eec53281466d7469acdde75446e96c9c4b2e2ebc493b7300ff4e8c2622080a0b0420d3e93e631647fbd24474769f4e03003d176a61f3d4012e5994062a17f3ea0e5238ae4400e927256247b79ad5999a1e6d49f69b318d9f179ba962285ddf3efa00e0f68936d608c0ef4c831ebd0635e5bf20c57cc79b6eea4083ecd21eeae3e6ea0165a0afc88dfd3a636d568bae74bd2cff21bf2f74c9e136bc6211f0c32e4e03ba0c7359fdf758df1a47c6027fe7450fc8daf80bdb3885c4219377ddab6a46d2364a02e63bdd28fe3c0e9a52970ba210698f43dd65b3f54523e8377e98b60ebbc79e380",
                        "0xf8d180a0675d893f6365176414dacccc2fcfee7953dacc5c17c3deef7ef5f05f9f850dff80a054123f3c71216be9dd9d9786425eb6d2e154d63b1b9c5a985a139601a9d5fe3da0c3f7ad7855707196662476d517e0957d7b8668fca2552fa2f662d044798aad7ea09f68b663d594cb87b914fa9c62468c8ed5d9056036e2a242b2c86bd9772e728280a001848a55098f7acee41d14af3a863c1aecf7438114b262bc109ea0b501a486a2808080a0cadb7df489a4ff4a703774bff124bb433c513c1d0a21f7f8fc01bcec2c9b9dce8080808080",
                        "0xe2a0205a850bd3eaa6019633d0f5c8a78c6c0c0de04d38b3fb3160af8e1b750e05695d"
                    ],
                    "value": "0x5d"
                },
                {
                    "key": "0xb35a850bd3eaa6019633d0f5c8a78c6c0c0de04d38b3fb3160af8e1b750e0569",
                    "proof": [
                        "0xf901d1a027903cc673323b319a5002734b747d98593df12d0e679616c23da9ee72dcd55280a06c306b7f0c26dd4592a3014d420575b0214e6f087d92093a3fcb639c00f0371fa0b395abe21b831935c13bfe163c01049b7ce15e1d68da00b62f65db626dc862ada0a0358c91d299929c41e64e0c79dc86293f63ff7d915a716474abd6dfb6a3baa7a0c4c5a4b95e6b0ca4f1112afc3bb9d067991a592a16bbf2aa234bd4c541b32420a01b4cdc62ee646cf3ff6b6a7b93e228d493c12ef47de828b21a9ecc503e96d319a0d38812229e6822be6d8997e2ce7e85685ebbde5998550650fb0439fc5eb5c66ba029e03eec53281466d7469acdde75446e96c9c4b2e2ebc493b7300ff4e8c2622080a0b0420d3e93e631647fbd24474769f4e03003d176a61f3d4012e5994062a17f3ea0e5238ae4400e927256247b79ad5999a1e6d49f69b318d9f179ba962285ddf3efa00e0f68936d608c0ef4c831ebd0635e5bf20c57cc79b6eea4083ecd21eeae3e6ea0165a0afc88dfd3a636d568bae74bd2cff21bf2f74c9e136bc6211f0c32e4e03ba0c7359fdf758df1a47c6027fe7450fc8daf80bdb3885c4219377ddab6a46d2364a02e63bdd28fe3c0e9a52970ba210698f43dd65b3f54523e8377e98b60ebbc79e380",
                        "0xf8718080a05e7546db3694ea1694d2a401154ab5196dd7f74de4a6fa5c7bac043d9ec4350da068a4e97d2e458599c669c76b55a1ba282b33ced3afbeb387d3c313dfca817865808080808080a07bc8b0ba719ce964b67fa59ef79567b40ca8041b821f7b8c3fac7eecc39ff7f4808080808080",
                        "0xf843a0204551c6741dedb7cfd930aad28d2f1daa295d0fedadeb1176ea90b43d38315ea1a06e657574726f6e317a38716a736d746a78636433366a306c6132727332726673"
                    ],
                    "value": "0x6e657574726f6e317a38716a736d746a78636433366a306c6132727332726673"
                },
                {
                    "key": "0xb35a850bd3eaa6019633d0f5c8a78c6c0c0de04d38b3fb3160af8e1b750e056a",
                    "proof": [
                        "0xf901d1a027903cc673323b319a5002734b747d98593df12d0e679616c23da9ee72dcd55280a06c306b7f0c26dd4592a3014d420575b0214e6f087d92093a3fcb639c00f0371fa0b395abe21b831935c13bfe163c01049b7ce15e1d68da00b62f65db626dc862ada0a0358c91d299929c41e64e0c79dc86293f63ff7d915a716474abd6dfb6a3baa7a0c4c5a4b95e6b0ca4f1112afc3bb9d067991a592a16bbf2aa234bd4c541b32420a01b4cdc62ee646cf3ff6b6a7b93e228d493c12ef47de828b21a9ecc503e96d319a0d38812229e6822be6d8997e2ce7e85685ebbde5998550650fb0439fc5eb5c66ba029e03eec53281466d7469acdde75446e96c9c4b2e2ebc493b7300ff4e8c2622080a0b0420d3e93e631647fbd24474769f4e03003d176a61f3d4012e5994062a17f3ea0e5238ae4400e927256247b79ad5999a1e6d49f69b318d9f179ba962285ddf3efa00e0f68936d608c0ef4c831ebd0635e5bf20c57cc79b6eea4083ecd21eeae3e6ea0165a0afc88dfd3a636d568bae74bd2cff21bf2f74c9e136bc6211f0c32e4e03ba0c7359fdf758df1a47c6027fe7450fc8daf80bdb3885c4219377ddab6a46d2364a02e63bdd28fe3c0e9a52970ba210698f43dd65b3f54523e8377e98b60ebbc79e380",
                        "0xf89180a0510fcf71326d98a9c5b9a45f6407dc3ce6959d0e108bb432ce2c2afe39a990e180a00bf5d9144ee88e66b9e83711295ae1c1f1b164a4ad109b2cd72f4d09fe458608808080808080a0c126cb8a53726df333e7a3d539524e8de5595f089eafd123511d8ec5e7faec74a001080472224688394a1525211b12f6a1fc618f046a1ed1cd88b2005cb0f5b64c8080808080",
                        "0xf843a02010056ea7655711a779f9e7125a88574457cbc2e124e9d9e9fd3fbce1753cb8a1a07466356e786d6164793268783861000000000000000000000000000000000000"
                    ],
                    "value": "0x7466356e786d6164793268783861000000000000000000000000000000000000"
                }
            ]
        },
        "root": "0xe1bc903054369d0b1c239c9b3f8445ef20ec0bb020e1eaf3e62af8174400b9f9",
        "withdraw": {
            "id": 0,
            "owner": "0x510c4a1d637ff374399826f421003b775dc3e8dc",
            "receiver": "neutron1z8qjsmtjxcd36j0la2rs2rfstf5nxmady2hx8a",
            "redemptionRate": "0x49e88a0",
            "sharesAmount": "0x26ab"
        }
    }"#;

    let data: Value = serde_json::from_str(data).unwrap();

    let withdraw = data["withdraw"].clone();
    let withdraw: WithdrawRequest = serde_json::from_value(withdraw).unwrap();

    let root = data["root"].as_str().unwrap().strip_prefix("0x").unwrap();
    let root = hex::decode(root).unwrap();

    let proof = data["proof"].clone();
    let proof: EIP1186AccountProofResponse = serde_json::from_value(proof).unwrap();

    clearing_queue_core::verify_proof(&proof, &withdraw, &root).unwrap();
}
