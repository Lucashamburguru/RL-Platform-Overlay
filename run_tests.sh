#!/usr/bin/env bash

set -euo pipefail

cargo test --locked --all-targets --all-features "$@"
