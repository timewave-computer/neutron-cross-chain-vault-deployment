# maxBTC mint Vault

This vault is for BTC LST tokens that are transferred directly from Ethereum to the Cosmos Hub (Gaia) using IBC Eureka without going through an intermediate domain. The tokens transferred should be depositable in the maxBTC contract to be able to get maxBTC.

## Phase 1 Flow

### Deposit Process

1. Users deposit BTC LST in the vault contract on Ethereum and receive vault shares
2. The strategist executes the coprocessor app, which generates a ZK proof and posts it to the authorization contract on Ethereum, triggering an Eureka Transfer with an empty memo via the IBCEurekaTransfer library. This sends all deposited BTC LST from Ethereum to an ICA on Gaia. The ZK proof ensures an empty memo to avoid extra hops and hardcodes the maximum bridge fee
3. Once funds arrive, the strategist executes an IBC transfer authorization to move assets from the ICA to the deposit account on Neutron
4. Once funds are in the deposit account, the strategist executes a maxBTC issue authorization that deposits the tokens in a maxBTC contract, sending the resulting maxBTC to the settlement account

### Withdraw Process

1. User requests a withdrawal on Ethereum, which is stored in the contract state with the current redemption rate and burned share amount
2. The strategist executes the coprocessor, which returns a ZK proof after state proof verification of the vault contract on Ethereum. This proof contains the amount of tokens the user is entitled to as public inputs
3. The strategist posts the proof to the authorization contract on Neutron, which executes a `register_obligation` message on the clearing queue.
4. The strategist triggers the obligation settlement on the clearing queue library, and the user receives maxBTC from the settlement account.
