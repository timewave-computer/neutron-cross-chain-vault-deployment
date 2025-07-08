# Coprocessor Apps

This folder contains the coprocessor apps that will be used for the different vaults. There are 3 different ones:

- Eureka transfer: this app is used to create the proofs for an Eureka transfer that limits the fees that can be charged for the transfer up to a certain amount. Additionally it forces an empty memo to make sure an additional IBC transfer is not triggered once it's received on the destination chain.

- Lombard transfer: modified version of Eureka transfer app. This app allows a memo but performs a strict validation on the memo, constraining the actions that can be added on the memo.

- Clearing queue: this app will create the proofs to register an obligation on a clearing queue. It does so by verifying the state proof of a vault withdrawal request and constructing the register obligation message that will eventually be sent to the clearing queue library.

## Compilation

#### Install Cargo Valence

A CLI helper is provided to facilitate the use of standard operations like deploying a domain, circuit, proving statements, and retrieving state information.

To install:

```bash
cargo install \
  --git https://github.com/timewave-computer/valence-coprocessor.git \
  --tag v0.3.1 \
  --locked cargo-valence
```

### Deploy the app

In root folder:

```bash
just compile <app_name>
```

Example:

```bash
just compile clearing-queue
```

or alternatively

```bash
    cargo-valence --socket prover.timewave.computer:37281 \
    deploy circuit \
    --controller ./coprocessor-apps/<app_name>/controller \
    --circuit <app_name>-circuit
```
