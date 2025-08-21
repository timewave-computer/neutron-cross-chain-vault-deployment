# Clearing queue Coprocessor app

Before deploying this app, make sure the correct `VAULT_ADDRESS` is set in /core/src/lib.rs and the correct
`SCALE_FACTOR` (10^deposit_token_decimals - e.g. `1000000` for vaults where you deposit USDC or `100000000` for vaults where you deposit WBTC. For vaults where the redemption rate is calculated in the same currency as the deposit token it will be equal to the initial redemption rate) and `CLEARING_QUEUE_LIBRARY_ADDRESS` are set in /circuit/src/lib.rs
