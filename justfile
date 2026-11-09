set shell := ["bash", "-euo", "pipefail", "-c"]

# List the project commands.
default:
    @just --list

check:
    just --fmt --check
    cargo fmt --check
    cargo deny --locked check licenses sources
    cargo shear --locked --deny-warnings

# Execution host or runner only.
test: build audit wasm-check
    cargo nextest run --locked --all-features --jobs 4 --profile ci
    cargo clippy --locked --all-targets --all-features --jobs 4 -- -D warnings
    cargo test --locked --doc --jobs 4

# Full acceptance needs a fresh owned backend/native credential fixture.
verify: check test wasm-check backend-acceptance
    cargo build --locked --release --jobs 4
    cargo audit --deny warnings

wasm-check:
    cargo clippy --locked --lib --features browser-bindings --target wasm32-unknown-unknown -- -D warnings

# Requires a private receipt and an isolated real native credential service.
backend-acceptance:
    cargo test --locked --features development-http --test backend -- --ignored

# Fetch locked source dependencies without compiling.
setup:
    cargo fetch --locked

# Compile the locked release workspace.
build:
    cargo build --locked --workspace --release --jobs 4

# Check current RustSec advisories.
audit:
    cargo audit --deny warnings

# Run the portable CI contract; native fixtures have explicit acceptance recipes.
ci-verify: check test build audit wasm-check
