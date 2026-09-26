@echo off
setlocal

echo Running formatting...
cargo fmt --all -- --check
if errorlevel 1 exit /b 1

echo Running Clippy (normal build, all targets)...
cargo clippy --locked --all-targets -- -D warnings
if errorlevel 1 exit /b 1

echo Running tests (normal build, all targets)...
cargo test --locked --all-targets
if errorlevel 1 exit /b 1

echo Running Clippy (all targets and features)...
cargo clippy --locked --all-targets --all-features -- -D warnings
if errorlevel 1 exit /b 1

echo Running tests (all targets and features)...
cargo test --locked --all-targets --all-features
if errorlevel 1 exit /b 1

echo Running Microsoft Store feature check...
cargo check --locked --all-targets --features microsoft-store
if errorlevel 1 exit /b 1

echo All checks passed.
exit /b 0
