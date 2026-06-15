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
        # Print up to 30 lines of error/warning lines to save tokens
        local filtered
        filtered=$(echo "$output" | grep -iE "error|warning|failed|aborting|diff" | head -n 30)
        if [ -n "$filtered" ]; then
            echo "$filtered"
        else
            echo "$output" | tail -n 15
        fi
        return $status
    else
        echo "✅ $cmd_name passed."
        return 0
    fi
}

status_fmt=0
status_clippy=0
status_test=0

# Run format check
run_cargo_cmd "cargo fmt" cargo fmt -- --check
status_fmt=$?

# Run clippy
run_cargo_cmd "cargo clippy" cargo clippy --all-targets -- -D warnings
status_clippy=$?

# Run tests using the existing tests script to keep output clean and token-friendly
echo "Running cargo test..."
test_output=$(./run_tests.sh 2>&1)
status_test=$?
if [ $status_test -ne 0 ]; then
    echo "❌ cargo test FAILED:"
    echo "$test_output" | grep -iE "failed|error|panic" | head -n 25
else
    echo "✅ cargo test passed."
fi

if [ $status_fmt -eq 0 ] && [ $status_clippy -eq 0 ] && [ $status_test -eq 0 ]; then
    echo "🎉 All checks passed successfully!"
    exit 0
else
    echo "⚠️ Some checks failed."
    exit 1
fi
