# BTC LST Vault

This vault is for BTC LST tokens that are transferred directly from Ethereum to the Cosmos Hub (Gaia) using IBC Eureka without going through an intermediate domain. Examples include pumpBTC, brBTC, and solvBTC. Note that LBTC (Lombard BTC) and WBTC are special cases with their own dedicated vaults.

The vault operates in two phases:

- **Phase 1** (pre maxBTC): BTC LST is paired with WBTC in the underlying supervault
- **Phase 2** (maxBTC available): BTC LST is paired with maxBTC in the underlying supervault, with the previous position migrated to the new supervault

## Phase 1 Flow

### Deposit Process

1. Users deposit BTC LST in the vault contract on Ethereum and receive vault shares
2. The strategist executes the coprocessor app, which generates a ZK proof and posts it to the authorization contract on Ethereum, triggering an Eureka Transfer with an empty memo via the IBCEurekaTransfer library. This sends all deposited BTC LST from Ethereum to an ICA on Gaia. The ZK proof ensures an empty memo to avoid extra hops and hardcodes the maximum bridge fee
3. Once funds arrive, the strategist executes an IBC transfer authorization to move assets from the ICA to the deposit account on Neutron
4. Once funds are in the deposit account, the strategist executes a split+lend+deposit authorization that lends a portion of tokens into a Mars position and deposits the remainder in a supervault, sending LP tokens to the settlement account

### Withdraw Process

1. User requests a withdrawal on Ethereum, which is stored in the contract state with the current redemption rate and burned share amount
2. The strategist executes the coprocessor, which returns a ZK proof after state proof verification of the vault contract on Ethereum. This proof contains the amount of tokens the user is entitled to as public inputs
3. The strategist posts the proof to the authorization contract on Neutron, which executes a `register_obligation` message on the clearing queue. This library splits the user's entitled amount into an array of BTC LST tokens and supervault LP tokens according to settlement ratios
4. The strategist withdraws sufficient tokens from the Mars lending position to pay the user from the settlement account
5. The strategist triggers the obligation settlement on the clearing queue library, and the user receives funds from the settlement account

![Phase 1 Flow](images/btc_lst_phase1.png)

## Phase Transition

**Important**: Before triggering the phase transition, the strategist must settle all current obligations.

This transition is executed by the program owner in a single authorization execution with the following steps:

1. Withdraw all liquidity from the BTCLST/wBTC supervault
2. Update the maxBTC issuer library with the correct maxBTC contract address
3. Issue maxBTC, consuming all wBTC and minting maxBTC sent to the supervault deposit account
4. Forward BTCLST using the phase shift forwarder from the settlement account to the supervault deposit account
5. Update supervault deposit library config with the new supervault (BTCLST/maxBTC) information
6. Update clearing queue library config with the new supervault information
7. Provide liquidity to the supervault with the maxBTC and BTCLST in the supervault deposit account

![Phase Transition](images/btc_lst_phase_transition.png)

## Phase 2 Flow

Phase 2 operates identically to Phase 1, but now deposits into a BTCLST/maxBTC supervault instead of BTCLST/wBTC.

![Phase 2 Flow](images/btc_lst_phase2.png)
