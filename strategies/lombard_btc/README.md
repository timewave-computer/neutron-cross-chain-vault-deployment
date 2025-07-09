# LBTC Vault

This vault is for LBTC tokens that are transferred from Ethereum to the Cosmos Hub (Gaia) via Lombard Ledger using IBC Eureka. This is done using a memo and the lombardTransfer function from the IBC Eureka Library. The memo is used to trigger a swap of the IBCeable LBTC vouchers (iLBTCv) to LBTC on Lombard chain and transfer them to the valence ICA on the Hub. This is needed because LBTC is not directly transferred to the Hub but instead mints/burns a voucher that needs to be sent to the Lombard Ledger and swapped to LBTC.

The vault consists of two phases with the following characteristics.

- Phase 1 (pre maxBTC): the underlying supervault pairs the LBTC with WBTC.
- Phase 2 (maxBTC available): the underlying supervault pairs the LBTC with maxBTC and the previous position is migrated to the new supervault.

## Phase 1 flow

### Deposit

1. Users deposit the LBTC in the vault contract on Ethereum and get vault shares.
2. The strategist executes the coprocessor which returns a ZK proof and posts it to the authorization contract on Ethereum that triggers an Eureka Transfer with a memo on the IBCEurekaTransfer library which sends all the deposited LBTC from Ethereum to the Lombard Ledger. During the EurekaTransfer, the LBTCs are burnt and a voucher (iLBTCv) is minted. The memo is used to swap the iLBTCv back to LBTC on Lombard and transfer them using IBC to the ICA sitting on the Cosmos Hub.
3. Once funds arrive, the strategist executes an IBC transfer authorization that transfers the assets from the ICA to the deposit account on Neutron.
4. Once funds are in the deposit account, the strategist executes a split+lend+deposit authorization that lends part of the tokens into a Mars position and deposits the other part in a supervault, sending the LP tokens to the settlement account.

### Withdraw

1. User requests a withdraw on Ethereum. This withdraw request is stored in the contract state with the current redemption rate and the amount of shares burned.
2. The strategist executes the coprocessor which returns a ZK proof after doing state proof verification of the vault contract on Ethereum. This proof contains, as public inputs, the amount of tokens that the user is entitled to.
3. The strategist posts the proof to the authorization contract on Neutron, which executes a `register_obligation` message on the clearing queue. This library splits the amount that the user should get in LBTC into an array of LBTC tokens and supervault LP tokens according to a settlement ratio.
4. The strategist withdraws enough tokens from the Mars lending position to pay the user from the settlement account.
5. The strategist triggers the obligation settlement on the clearing queue library and user gets the funds from the settlement account.

Here is a general diagram of the flow during phase 1:
![Phase 1](images/lbtc_phase1.png)

## Phrase transition

**NOTES**:
Before triggering the phase transition, the strategist must settle all current obligations.
This transition is executed by the program owner in a single authorization execution.

The program owner will execute the phase shift authorization with the following actions:

1. Withdraw all liquidity from the LBTC/wBTC supervault
2. Update the maxBTC issuer library with the correct maxBTC contract address.
3. Issue maxBTC, which will consume all wBTC and mint maxBTC that is sent to the supervault deposit account.
4. Forward the LBTC using the phase shift forwarder from the settlement account to the supervault deposit account.
5. Update supervault deposit library config with the new supervault (LBTC/maxBTC) information.
6. Update clearing queue library config with the new supervault information.
7. Provide liquidity to the supervault with the maxBTC and LBTC that are sitting in the supervault deposit account.

Here is a diagram of the phase transition:
![Phase transition](images/lbtc_phase_transition.png)

## Phase 2 flow

The phase 2 flow is exactly the same as phase 1 but now instead of depositing into a LBTC/wBTC supervault we deposit into a LBTC/maxBTC one.

Here is a diagram for phase 2:
![Phase 2](images/lbtc_phase2.png)
