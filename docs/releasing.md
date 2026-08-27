# Release process

The repository uses `rust-toolchain.toml` and `Cargo.lock` as the canonical Rust
toolchain and dependency inputs. Local checks, CI, and release builds all run
`./check_all.sh`.

## Prepare the release

1. Put user-facing changes under `## [Unreleased]` in `CHANGELOG.md`.
2. Commit all feature, documentation, and changelog changes.
3. Confirm the working tree is clean.
4. Preview the release with `./release.sh --dry-run`.
5. Run `./release.sh --bump patch`, changing the bump type when appropriate.

The helper promotes the Unreleased notes into a dated version section, updates
`Cargo.toml` and the root package entry in `Cargo.lock`, runs the canonical
checks, creates a release commit, creates an annotated tag, and pushes only the
current branch and that exact tag. Use `--no-push` to inspect the commit and tag
locally before publishing.

Pushing the version tag starts the release workflow. Dependency auditing must
pass before Linux and Windows builds begin. Both builds upload signed binaries
and checksums; one final job downloads all artifacts and creates the GitHub
release using the matching changelog section.

Temporary audit exceptions and their review dates are tracked in
`docs/security-advisories.md`.
