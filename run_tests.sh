#!/bin/bash

# Run cargo test and capture stdout/stderr
output=$(cargo test "$@" 2>&1)
status=$?

if [ $status -eq 0 ]; then
    echo "✅ All cargo tests passed successfully."
    # Filter output to show only binary headers and final test results
    echo "$output" | grep -E "^[[:space:]]*Running|^[[:space:]]*test result:"
else
    echo "❌ cargo test FAILED (exit code $status):"
    echo "$output"
    exit $status
fi
