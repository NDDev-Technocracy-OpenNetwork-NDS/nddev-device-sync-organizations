set shell := ["bash", "-euo", "pipefail", "-c"]

check:
    cargo fmt --check
    cargo deny --locked check licenses sources
    cargo shear --locked --deny-warnings

# Execution host or runner only.
test:
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

# GitHub's runners lack the credential-store build inputs this workspace links; CI runs this
# before the checks (setup: true).
setup:
    if [ "${GITHUB_ACTIONS:-}" = true ] && [ "${RUNNER_ENVIRONMENT:-}" = github-hosted ]; then sudo apt-get update -qq && sudo apt-get install -qq --no-install-recommends -y libsecret-1-dev; fi
