@echo off
setlocal enabledelayedexpansion

set status_fmt=0
set status_clippy=0
set status_check=0

echo Running cargo fmt...
:: Run cargo fmt and check for errors
cargo fmt -- --check >nul 2>&1
if !errorlevel! neq 0 (
    echo ❌ cargo fmt FAILED:
    cargo fmt -- --check
    set status_fmt=1
) else (
    echo ✅ cargo fmt passed.
)

echo Running cargo clippy...
:: Capture clippy output to temp file
set CLIPPY_TEMP=%TEMP%\cargo_clippy_out.tmp
cargo clippy --all-targets -- -D warnings > "%CLIPPY_TEMP%" 2>&1
if !errorlevel! neq 0 (
    echo ❌ cargo clippy FAILED:
    type "%CLIPPY_TEMP%"
    set status_clippy=1
) else (
    echo ✅ cargo clippy passed.
)
del "%CLIPPY_TEMP%" 2>nul

echo Running cargo check...
:: Capture cargo check output to temp file
set CHECK_TEMP=%TEMP%\cargo_check_out.tmp
cargo check > "%CHECK_TEMP%" 2>&1
if !errorlevel! neq 0 (
    echo ❌ cargo check FAILED:
    type "%CHECK_TEMP%"
    set status_check=1
) else (
    echo ✅ cargo check passed.
)
del "%CHECK_TEMP%" 2>nul

if %status_fmt%==0 if %status_clippy%==0 if %status_check%==0 (
    echo 🎉 All checks passed successfully!
    exit /b 0
)

echo ⚠️ Some checks failed.
exit /b 1
