set shell := ["bash", "-euo", "pipefail", "-c"]

# Checks source and command formatting without compilation.
check:
    just --fmt --check
    cargo fmt --check
    cargo deny --locked check licenses sources
    cargo shear --locked --deny-warnings

# Execution host or runner only.
test:
    cargo nextest run --locked --all-features --jobs 4 --profile ci
    cargo clippy --locked --all-targets --all-features --jobs 4 -- -D warnings
    cargo test --locked --doc --jobs 4

# Full acceptance needs a fresh owned backend/native credential fixture.
verify: check test wasm-check ci-backend-acceptance
    cargo build --locked --release --jobs 4
    cargo audit --deny warnings

wasm-check:
    cargo clippy --locked --lib --features browser-bindings --target wasm32-unknown-unknown -- -D warnings

# Requires a private receipt and an isolated real native credential service.
backend-acceptance:
    cargo test --locked --features development-http --test backend -- --ignored

# Runs the same backend case under Accounts owned server and native-store fixture.
ci-backend-acceptance:
    python3 tools/with_backend.py

# Fetches declared fixture tools and prepares native credential inputs on hosted Linux.
setup:
    git submodule update --init tests/fixtures/accounts
    python3 tools/setup_ci.py
