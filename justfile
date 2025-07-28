start strategy:
    RUST_LOG=info cargo run -p {{strategy}}_strategist --bin runner
    
compile circuit:
    cargo-valence --socket https://service.coprocessor.valence.zone \
    deploy circuit \
    --controller ./coprocessor-apps/{{circuit}}/controller \
    --circuit {{circuit}}-circuit

neutron-upload:
    cargo run --package packages --bin neutron_upload
