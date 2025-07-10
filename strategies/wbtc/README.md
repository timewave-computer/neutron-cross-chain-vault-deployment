# wBTC Vault

This vault is for wBTC tokens that are transferred directly from Ethereum to the Cosmos Hub (Gaia) using IBC Eureka without going through an intermediate domain.

The vault operates in two phases:

- **Phase 1** (pre maxBTC): wBTC is paired with multiple BTC LSTs across 6 supervaults
- **Phase 2** (maxBTC available): wBTC is paired with all previous BTC LSTs plus maxBTC across 7 supervaults

## Phase 1 Flow

### Deposit Process

1. Users deposit wBTC in the vault contract on Ethereum and receive vault shares
2. The strategist executes the coprocessor, which generates a ZK proof and posts it to the authorization contract on Ethereum, triggering an Eureka Transfer with an empty memo via the IBCEurekaTransfer library. This sends all deposited wBTC from Ethereum to an ICA on Gaia. The ZK proof ensures an empty memo to avoid extra hops and hardcodes the maximum bridge fee
3. Once funds arrive, the strategist executes an IBC transfer authorization to move assets from the ICA to the deposit account on Neutron
4. Once funds are in the deposit account, the strategist executes a split+lend+deposit authorization that lends a portion of tokens into a Mars position and deposits the remainder across multiple supervaults, sending LP tokens to the settlement account

### Withdraw Process

1. User requests a withdrawal on Ethereum, which is stored in the contract state with the current redemption rate and burned share amount
2. The strategist executes the coprocessor, which returns a ZK proof after state proof verification of the vault contract on Ethereum. This proof contains the amount of tokens the user is entitled to as public inputs
3. The strategist posts the proof to the authorization contract on Neutron, which executes a `register_obligation` message on the clearing queue. This library splits the user's entitled amount into an array of wBTC tokens and supervault LP tokens according to settlement ratios
4. The strategist withdraws sufficient tokens from the Mars lending position to pay the user from the settlement account
5. The strategist triggers the obligation settlement on the clearing queue library, and the user receives funds from the settlement account

![Phase 1 Flow](images/wbtc_phase1.png)

## Phase Transition

**Important**: Before triggering the phase transition, the strategist must settle all current obligations.

This transition is executed by the program owner in multiple authorization transactions due to its complexity.

![Phase Transition](images/wbtc_phase_transition.png)

## Phase 2 Flow

Phase 2 operates identically to Phase 1, with the addition of a wBTC/maxBTC supervault deposit. The strategist now executes 7 deposit messages instead of 6.

![Phase 2 Flow](images/wbtc_phase2.png)
