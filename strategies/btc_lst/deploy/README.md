# Deploy instructions

1. If the wasm blobs are not uploaded, run `just neutron-upload`. This will upload all contracts in /packages/src/contracts/cw and output the code ids in `neutron_code_ids.toml`. This file will be used to instantiate the contracts.

2. Fill in all the information in `neutron.toml` except the coprocessor app fields at the end and run the `neutron_deploy.rs` script which will instantiate all the contracts, trigger the ICA creation and output all relevant addresses in `neutron_strategy_config.toml` and `gaia_strategy_config.toml` which will be used by the strategist.

3. Deploy on Ethereum running `ethereum_deploy.rs`. We'll be transfering to the ICA created on the Cosmos Hub, the coprocessor app will restrict an empty memo to avoid sending anywhere else.

4. Now that we have deployed on both Ethereum and Neutron we can finalize the coprocessor apps and get their relevant IDs.

5. Run `neutron_initialization.rs` which will create all the authorizations including the ZK authorization.

6. Run `ethereum_initialization.rs` which will create the relevant IBC Eureka ZK Authorization on the authorization contract that the strategist can execute.
