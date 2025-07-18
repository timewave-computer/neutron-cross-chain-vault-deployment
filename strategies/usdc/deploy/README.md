# Deploy instructions

1. If the wasm blobs are not uploaded, run `just neutron-upload`. This will upload all contracts in /packages/src/contracts/cw and output the code ids in `neutron_code_ids.toml`. This file will be used to instantiate the contracts.

2. Fill in all the information in `neutron.toml` except the coprocessor app fields at the end and run the `neutron_deploy.rs` script which will instantiate all the contracts. After everything is deployed, we have to create a forwarding account on noble that will automatically transfer the USDC from noble to our neutron deposit account. This can be done like this:

```bash
TX_HASH=$(./nobled tx forwarding register-account channel-18 neutron1g7498csr6svpfxtu26kk8tq5mavdfpr5j5p6h4cmtm9tv27gjlcqupl3kr --chain-id noble-1 --node https://noble-rpc.polkachu.com --fees 20000uusdc --from <KEY> --output json --yes | jq -r '.txhash')

./nobled query tx $TX_HASH --node https://noble-rpc.polkachu.com
```

We will need to convert this new noble bech32 address into a bytes32 address and input that in the `recipient` field of `ethereum.toml` which will now be a correct 20 bytes address (padded with 0s).

Set it also in `noble_strategy_config.toml` so that the strategist can query this account if needed.

3. Deploy on Ethereum running `ethereum_deploy.rs`. 

4. Now that we have deployed on both Ethereum and Neutron we can finalize the Neutron coprocessor app and get the ID.

5. Add the ID to `neutron_strategy_config.toml` and `neutron.toml` in the relevant coprocessor_app_id field.

6. Run `neutron_initialization.rs` which will create all the authorizations including the ZK authorization.

7. Run `ethereum_initialization.rs` which will create the standard authorization. There's no ZK flow for Ethereum because we are using CCTP transfer.
