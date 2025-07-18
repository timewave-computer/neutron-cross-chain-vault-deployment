# Deploy instructions

1. If the wasm blobs are not uploaded, run `just neutron-upload`. This will upload all contracts in /packages/src/contracts/cw and output the code ids in `neutron_code_ids.toml`. This file will be used to instantiate the contracts.

2. Fill in all the information in `neutron.toml` except the coprocessor app fields at the end and run the `neutron_deploy.rs` script which will instantiate all the contracts, trigger the ICA creation and output all relevant addresses in `neutron_strategy_config.toml` and `gaia_strategy_config.toml` which will be used by the strategist.

3. Deploy on Ethereum running `ethereum_deploy.rs`. Note that we don't need to transfer to the ICA from Eureka because we'll do that using a memo. We'll be transfering to the contract on Lombard that will trigger the actions to swap the iLBTCv to LBTC and forward them to the ICA we created on the Hub.

4. Now that we have deployed on both Ethereum and Neutron we can finalize the coprocessor apps and get their relevant IDs.

5. Add the IDs to `neutron_strategy_config.toml`, `neutron.toml`, `ethereum_strategy_config.toml` and `ethereum.toml` in the relevant fields.

6. Run `neutron_initialization.rs` which will create all the authorizations including the ZK authorization.

7. Run `ethereum_initialization.rs` which will create the relevant IBC Eureka ZK Authorization on the authorization contract that the strategist can execute.
