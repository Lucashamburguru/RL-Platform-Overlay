#!/usr/bin/env bash

set -euo pipefail

run_check() {
    local label="$1"
    shift

    echo "Running ${label}..."
    "$@"
    echo "Passed ${label}."
}

run_check "formatting" cargo fmt --all -- --check
run_check "Clippy (all targets and features)" \
    cargo clippy --locked --all-targets --all-features -- -D warnings
run_check "tests (all targets and features)" \
    cargo test --locked --all-targets --all-features
run_check "Microsoft Store feature check" \
    cargo check --locked --all-targets --features microsoft-store

echo "All checks passed."
