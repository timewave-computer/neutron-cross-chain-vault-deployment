# USDC LP Vault

This vault is for USDC tokens that are transferred from Ethereum to Neutron via Noble using CCTP (Cross-Chain Transfer Protocol). The transfer is facilitated by a CCTPTransfer library that sends USDC to a forwarding account on Noble, which automatically IBCs the funds to the Neutron deposit account upon receipt.

Unlike other vaults, this vault operates in a single phase with no transitions.

## Deposit Process

1. Users deposit USDC in the vault contract on Ethereum and receive vault shares
2. The strategist executes the transfer authorization on the authorization contract, triggering the transfer via the CCTPTransfer library
3. Once funds arrive in the deposit account, the strategist executes a deposit authorization that deposits the USDC into the USDC/BTC supervault, sending LP tokens to the settlement account

## Withdraw Process

1. User requests a withdrawal on Ethereum, which is stored in the contract state with the current redemption rate and burned share amount
2. The strategist executes the coprocessor, which returns a ZK proof after state proof verification of the vault contract on Ethereum. This proof contains the amount of tokens the user is entitled to as public inputs
3. The strategist posts the proof to the authorization contract on Neutron, which executes a `register_obligation` message on the clearing queue. This library calculates how many LP tokens the user will receive based on current price by querying the supervault
4. The strategist triggers the obligation settlement on the clearing queue library, and the user receives the corresponding LP shares from the settlement account

![USDC LP Vault Flow](images/usdc_lp_vault.png)
