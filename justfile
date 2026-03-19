# Default task
default: check

# Quick check: format and lint
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    taplo fmt --check

# Full CI: format, lint, test, spell, build examples
ci: check test spell build

# Format code
fmt:
    cargo fmt --all
    taplo fmt

# Run all tests
test:
    cargo test --workspace --all-features

# Lint with warnings as errors
lint:
    cargo clippy --workspace --all-targets --all-features -- -D warnings

# Build workspace including examples
build:
    cargo build --workspace --all-features --examples

# Build docs (with all features to show everything)
doc:
    cargo doc --workspace --all-features --no-deps

# Open docs in browser
doc-open:
    cargo doc --workspace --all-features --no-deps --open

# Run spell checker
spell:
    cargo spellcheck

# Run all examples
examples:
    cargo run --example hello-world
    cargo run --example pingpong
    cargo run --example guesser
    cargo run --example monitoring --features monitoring
    cargo run --example arbitrage --features test-harness
    cargo run --example backpressure --features monitoring

# Clean build artifacts
clean:
    cargo clean

# Watch and run tests on changes (requires bacon)
watch:
    bacon test

# Check if `uv` is installed. See installation instructions https://docs.astral.sh/uv/getting-started/installation/
check-uv:
    @if ! command -v uv >/dev/null 2>&1; then echo "Requires \`uv\`. See installation instructions https://docs.astral.sh/uv/getting-started/installation/"; exit 1; fi

# Run Actor->Broker->Actor transport benchmark and print throughput matrix.
# Print throughput matrix (messages/sec) from Criterion estimates.
bench-a2a: check-uv
    cargo bench -p maiko --bench actor_to_actor_transport_maiko -- --noplot
    uv run --project scripts scripts/bench_a2a_matrix.py

# Run all Actor->Actor transport benches, then compare throughput across frameworks.
bench-a2a-compare: check-uv
    cargo bench --profile bench -p maiko --bench actor_to_actor_transport_maiko -- --noplot
    cargo bench --profile bench -p maiko --bench actor_to_actor_transport_actix -- --noplot
    cargo bench --profile bench -p maiko --bench actor_to_actor_transport_ractor -- --noplot
    uv run --project scripts scripts/bench_a2a_compare.py
