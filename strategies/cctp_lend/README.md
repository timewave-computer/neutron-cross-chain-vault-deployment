# CCTP Lending Vault

This vault is for USDC tokens that are transferred from Ethereum to Neutron via Noble using CCTP (Cross-Chain Transfer Protocol). The transfer is facilitated by a CCTPTransfer library that sends USDC to a forwarding account on Noble, which automatically IBCs the funds to the Neutron deposit account upon receipt.

Unlike other vaults, this vault operates in a single phase with no transitions.

## Deposit Flow

1. **Deposit on Ethereum**: Users deposit USDC into the `OneWayVault` contract on Ethereum and receive vault shares in return.
2. **CCTP Transfer**: The strategist triggers a CCTP transfer, moving the USDC from the `OneWayVault`'s `depositAccount` on Ethereum to a specified recipient address on Noble.
3. **IBC-Autoforward from Noble to Neutron**: From Noble, the USDC is transferred via IBC to the `deposit` account on Neutron via Noble's `forwarding` module.
4. **Lend on Mars Protocol**: Once the funds arrive on Neutron, the strategist executes a transaction to lend the USDC on Mars Protocol.

## Withdrawal Flow

1. User requests a withdrawal on Ethereum, which is stored in the contract state with the current redemption rate and burned share amount
2. The strategist executes the coprocessor, which returns a ZK proof after state proof verification of the vault contract on Ethereum. This proof contains the amount of tokens the user is entitled to as public inputs
3. The strategist posts the proof to the authorization contract on Neutron, which executes a `register_obligation` message on the clearing queue. This library calculates how many deposit tokens the user will receive based on current price by querying the Mars position
4. After withdrawing the necessary amount of liquidity from Mars position, the strategist triggers the obligation settlement on the clearing queue library, and the user receives the corresponding deposit tokens from the settlement account
