#!/bin/bash

# Suppress cargo output unless there is an error
run_cargo_cmd() {
    local cmd_name="$1"
    shift
    echo "Running $cmd_name..."
    
    # Run command and capture both stdout and stderr
    local output
    output=$( "$@" 2>&1 )
    local status=$?
    
    if [ $status -ne 0 ]; then
        echo "❌ $cmd_name FAILED (exit code $status):"
        echo "$output"
        return $status
    else
        echo "✅ $cmd_name passed."
        return 0
    fi
}

status_fmt=0
status_clippy=0
status_check=0

# Run format check
run_cargo_cmd "cargo fmt" cargo fmt -- --check
status_fmt=$?

# Run clippy
run_cargo_cmd "cargo clippy" cargo clippy --all-targets -- -D warnings
status_clippy=$?

# Run cargo check
run_cargo_cmd "cargo check" cargo check
status_check=$?

if [ $status_fmt -eq 0 ] && [ $status_clippy -eq 0 ] && [ $status_check -eq 0 ]; then
    echo "🎉 All checks passed successfully!"
    exit 0
else
    echo "⚠️ Some checks failed."
    exit 1
fi
