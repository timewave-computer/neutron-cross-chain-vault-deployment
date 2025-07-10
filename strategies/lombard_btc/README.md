# LBTC Vault

This vault is for LBTC tokens that are transferred from Ethereum to the Cosmos Hub (Gaia) via Lombard Ledger using IBC Eureka. The transfer uses a memo and the `lombardTransfer` function from the IBC Eureka Library. The memo triggers a swap of IBCeable LBTC vouchers (iLBTCv) to LBTC on the Lombard chain and transfers them to the valence ICA on the Hub. This process is necessary because LBTC is not directly transferred to the Hub; instead, it mints/burns vouchers that must be sent to the Lombard Ledger and swapped to LBTC.

The vault operates in two phases:

- **Phase 1** (pre maxBTC): LBTC is paired with WBTC in the underlying supervault
- **Phase 2** (maxBTC available): LBTC is paired with maxBTC in the underlying supervault, with the previous position migrated to the new supervault

## Phase 1 Flow

### Deposit Process

1. Users deposit LBTC in the vault contract on Ethereum and receive vault shares
2. The strategist executes the coprocessor, which generates a ZK proof and posts it to the authorization contract on Ethereum, triggering an Eureka Transfer with a memo via the IBCEurekaTransfer library. This sends all deposited LBTC from Ethereum to the Lombard Ledger. During the EurekaTransfer, LBTC is burned and vouchers (iLBTCv) are minted. The memo includes an instruction to swap the iLBTCv back to LBTC on Lombard and transfers them via IBC to the ICA on the Cosmos Hub
3. Once funds arrive, the strategist executes an IBC transfer authorization to move assets from the ICA to the deposit account on Neutron
4. Once funds are in the deposit account, the strategist executes a split+lend+deposit authorization that lends a portion of tokens into a Mars position and deposits the remainder in a supervault, sending LP tokens to the settlement account

### Withdraw Process

1. User requests a withdrawal on Ethereum, which is stored in the contract state with the current redemption rate and burned share amount
2. The strategist executes the coprocessor, which returns a ZK proof after state proof verification of the vault contract on Ethereum. This proof contains the amount of tokens the user is entitled to as public inputs
3. The strategist posts the proof to the authorization contract on Neutron, which executes a `register_obligation` message on the clearing queue. This library splits the user's entitled amount into an array of LBTC tokens and supervault LP tokens according to settlement ratios
4. The strategist withdraws sufficient tokens from the Mars lending position to pay the user from the settlement account
5. The strategist triggers the obligation settlement on the clearing queue library, and the user receives funds from the settlement account

![Phase 1 Flow](images/lbtc_phase1.png)

## Phase Transition

**Important**: Before triggering the phase transition, the strategist must settle all current obligations.

This transition is executed by the program owner in a single authorization execution with the following steps:

1. Withdraw all liquidity from the LBTC/wBTC supervault
2. Update the maxBTC issuer library with the correct maxBTC contract address
3. Issue maxBTC, consuming all wBTC and minting maxBTC sent to the supervault deposit account
4. Forward LBTC using the phase shift forwarder from the settlement account to the supervault deposit account
5. Update supervault deposit library config with the new supervault (LBTC/maxBTC) information
6. Update clearing queue library config with the new supervault information
7. Provide liquidity to the supervault with the maxBTC and LBTC in the supervault deposit account

![Phase Transition](images/lbtc_phase_transition.png)

## Phase 2 Flow

Phase 2 operates identically to Phase 1, but now deposits into a LBTC/maxBTC supervault instead of LBTC/wBTC.

![Phase 2 Flow](images/lbtc_phase2.png)
