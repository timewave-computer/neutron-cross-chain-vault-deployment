# Deploy instructions

1. If the wasm blobs are not uploaded, run `just neutron-upload`. This will upload all contracts in /packages/src/contracts/cw and output the code ids in `neutron_code_ids.toml`. This file will be used to instantiate the contracts.

2. Fill in all the information in `neutron.toml` except the coprocessor app fields at the end and run the `neutron_deploy.rs` script which will instantiate all the contracts, trigger the ICA creation and output all relevant addresses in `neutron_strategy_config.toml` and `gaia_strategy_config.toml` which will be used by the strategist.

3. Now that we have deployed on Neutron and we have all relevant addresses in `neutron_strategy_config.toml` and `gaia_strategy_config.toml` we can deploy on Ethereum. For that, input the generated ICA in `ethereum.toml` and run the `ethereum_deploy.rs` script.

4. Now that we have deployed on both Ethereum and Neutron we can finalize the coprocessor apps and get their relevant IDs.

5. Add the IDs to `neutron_strategy_config.toml`, `neutron.toml`, `ethereum_strategy_config.toml` and `ethereum.toml` in the relevant fields.

6. Run `neutron_initialization.rs` which will create all the authorizations including the ZK authorization.

7. Run `ethereum_initialization.rs` which will create the relevant IBC Eureka ZK Authorization on the authorization contract that the strategist can execute.

### Notes for Strategist

This vault involves updating all the supervaults LPers configuration for the 2nd phase (maxBTC migration). Therefore, once the migration is completed, the corresponding LP denoms of the supervaults need to be updated because they will be different as we are using different supervaults.
So, before relayer is restarted after the phase migration, the corresponding strategy_config files need to be updated with the right parameters.

