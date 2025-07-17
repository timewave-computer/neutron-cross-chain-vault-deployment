# Deployment Getting Started Guide

This guide provides a high-level overview and step-by-step instructions for deploying a new cross-chain vault strategy. The process involves setting up configurations, deploying contracts on Neutron and Ethereum, and initializing the necessary authorizations.

## Prerequisites

You will need a funded wallet mnemonic for deploying contracts.
Create a `.env` file from `.env.example` and set your `MNEMONIC`.

## General Deployment Flow

The deployment is a multi-step process that must be followed in order. On a high level, the flow is:

1.  **Configuration:** Prepare the necessary `.toml` configuration files for the specific strategy you are deploying.
2.  **Upload Neutron Contracts:** Upload the generic WASM contract blobs to the Neutron network. This provides the `code_ids` needed for instantiation. This only needs to be done once.
3.  **Deploy on Neutron:** Instantiate the core logic contracts on Neutron and register the Interchain Account(s) on relevant chains. This step is a prerequisite for the Ethereum deployment.
4.  **Deploy on Ethereum:** Deploy the vault and other Valence contracts on Ethereum.
5.  **Initialize Authorizations:** Set up the permissions and authorizations on both chains to allow the strategist and other components to interact with the contracts.

After these steps, the on-chain side of the vaults is ready to be managed by the strategist.

---

## Step-by-Step Instructions

### Step 1: Configuration

Before deploying, you must configure the parameters for your chosen strategy.

1.  **Create `.env` file:**
    ```bash
    cp .env.example .env
    ```
    Edit the `.env` file and add your wallet `MNEMONIC`.

2.  **Fill in `.toml` files:** Navigate to the strategy's deployment directory (e.g., `strategies/wbtc/deploy/src/`) and fill in the required parameters in:
    - `neutron.toml`: Contains parameters for the Neutron side, such as GRPC endpoints, chain ID, owner addresses, and asset information.
    - `ethereum.toml`: Contains parameters for the EVM side, such as RPC URL, owner addresses, and vault settings.

### Step 2: Upload Neutron WASM Contracts

This step uploads the compiled smart contracts to Neutron. You only need to do this once if the contracts haven't changed.

```bash
just neutron-upload
```

This command reads the contracts from `/packages/src/contracts/cw`, uploads them, and creates a `neutron_code_ids.toml` file in the `packages/src/contracts` directory with the resulting code IDs. This file will be read by all strategies during the deployment phase as for the most part they rely on the same on-chain contracts.

### Step 3: Deploy on Neutron

This script instantiates the contracts on Neutron using the code IDs from the previous step and creates the ICA(s) on relevant chain(s).

```bash
# Replace <strategy-name>
cargo run --bin neutron_deploy -p <strategy-name>-deploy
```

This step will produce the following:
- `neutron_strategy_config.toml`: Contains the addresses of the newly deployed Neutron contracts.
- `gaia_strategy_config.toml` or `noble_strategy_config.toml`: Contains the generated ICA address and other relevant information.

### Step 4: Deploy on Ethereum

This script deploys the contracts on Ethereum.

- **For `wbtc`:** You must first copy the `ica_address` from the `gaia_strategy_config.toml` generated in the previous step into the `ethereum.toml` file, placing it under `eureka_transfer.recipient`.

- **For `usdc` and `cctp_lend`:** The ICA address is passed differently (as bytes32), and the script handles the conversion.

Once the configuration is ready, run the deployment script:

```bash
# Replace <strategy-name>
cargo run --bin ethereum_deploy -p <strategy-name>-deploy
```

This will create an `ethereum_strategy_config.toml` file with the addresses of the deployed Ethereum contracts.

### Step 5: Deploy Coprocessor Apps

Each vault requires one or two (depending on the vault) coprocessor app deployed. We can deploy these by following the instructions in the coprocessor-apps folder. Once the coprocessor-app is deployed, we set the ID obtained after compilation in the corresponding field in `neutron.toml` and/or `ethereum.toml`.

Example:

For the LBTC vault we need a lombard-transfer and a clearing-queue coprocessor-app deployed. After these are deployed correctly, we need to set the fields:

In `neutron.toml`:

```toml
[coprocessor_app]
clearing_queue_coprocessor_app_id = "<clearing_queue_coprocessor_app_id>"
```

In `ethereum.toml`:

```toml
[coprocessor_app]
eureka_transfer_coprocessor_app_id = "<lombard_transfer_coprocessor_app_id>"
```

### Step 6: Initialize Authorizations

After all contracts are deployed on both chains, you must set up the authorizations that define who can perform what actions.

1.  **Initialize Neutron Authorizations:**
    ```bash
    # Replace <strategy-name>
    cargo run --bin neutron_initialization -p <strategy-name>-deploy
    ```

2.  **Initialize Ethereum Authorizations:**
    ```bash
    # Replace <strategy-name>
    cargo run --bin ethereum_initialization -p <strategy-name>-deploy
    ```

With that, the on-chain setup is complete. The strategist is ready to operate the vault.
