# Release Signing

Automatic Windows updates require two checks before the downloaded executable is staged:

- `rl-platform-overlay.exe.sha256` must match the downloaded executable.
- `rl-platform-overlay.exe.sig` must be a valid Ed25519 signature for the executable.

The app pins the Ed25519 public key in `src/update.rs`. The matching private seed must be stored as the GitHub Actions repository secret:

```text
RL_OVERLAY_RELEASE_SIGNING_KEY_B64
```

The secret value is a base64-encoded 32-byte Ed25519 seed.

## Signing Flow

The release workflow runs:

```bash
cargo run --release --bin sign_update -- --asset target/release/rl-platform-overlay.exe --out target/release/rl-platform-overlay.exe.sig
```

The helper reads `RL_OVERLAY_RELEASE_SIGNING_KEY_B64`, signs the asset bytes, and writes a base64 signature file.

## Key Rotation

Generate a new 32-byte seed with a trusted local tool, then print its public key:

```bash
export RL_OVERLAY_RELEASE_SIGNING_KEY_B64="<base64 seed>"
cargo run --bin sign_update -- print-public
```

Update `RELEASE_SIGNING_PUBLIC_KEY_B64` in `src/update.rs` and replace the GitHub secret with the new seed. Release the public-key update before relying on the new signing key for future auto-updates.
