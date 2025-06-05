# Deploy instructions

1. If the wasm blobs are not uploaded, run the `neutron_upload.rs` script which will upload all contracts in /deploy/contracts/cw and output the code ids in `neutron_code_ids.toml`. These file will be used to instantiate the file.

2. Fill in all the information in `neutron.toml` except the coprocessor app fields at the end and run the `neutron_deploy.rs` script which will instantiate all the contracts and output all relevant addresses in `neutron_strategy_config.toml` which will be used by the strategist.

3. Now that you have the contracts you should be able to use those addresses to finalize the coprocessor apps, compile them and get the relevant VKs and IDs. Input them in `neutron.toml`

4. Run `neutron_initialization.rs` which will create all the authorizations and initialize the ICA. It will output a `gaia_strategy_config.toml` file that will be used by the strategist.

5. Now that we have all the Cosmos part set up. Fill in all relevant fields in `ethereum.toml` except the coprocessor app fields and run `ethereum_deploy.rs`. This will output the `ethereum_strategy_config.toml` that strategist will use.

6. Now that we have all the addresses, we can finalize the IBC Eureka coprocessor app with the relevant IBC transfer contract address.

7. Run `ethereum_initialization.rs` which will create the relevant IBC Eureka ZK Authorization on the authorization contract that the strategist can execute.
