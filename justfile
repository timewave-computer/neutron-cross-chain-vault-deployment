start strategy:
    RUST_LOG=info cargo run -p {{strategy}}_strategist --bin runner
    
compile circuit:
    cargo-valence --socket prover.timewave.computer:37281 \
    deploy circuit \
    --controller ./coprocessor-apps/{{circuit}}/controller \
    --circuit {{circuit}}-circuit
