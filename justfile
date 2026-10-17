set shell := ["bash", "-euo", "pipefail", "-c"]

check:
    cargo fmt --check
    cargo nextest run --locked --all-features --jobs 4
    cargo clippy --locked --all-targets --all-features --jobs 4 -- -D warnings
    cargo audit --deny warnings
    cargo deny --locked check licenses sources

wasm-check:
    cargo clippy --locked --lib --features browser-bindings --target wasm32-unknown-unknown -- -D warnings

# Requires a private receipt and an isolated real native credential service.
backend-acceptance:
    cargo test --locked --features development-http --test backend -- --ignored
