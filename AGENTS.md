# RL Platform Overlay

Rust desktop companion for Rocket League, using egui/eframe, Tokio, immutable
state snapshots, and SQLite. Windows is the primary distribution platform;
development also happens on Linux. Follow the user's requested scope and preserve
unrelated working-tree changes.

## Read the relevant context

- [README.md](README.md): features, setup, build prerequisites and debug tools.
- [docs/architecture.md](docs/architecture.md): runtime boundaries, state ownership,
  concurrency, persistence and Windows window behavior.
- [docs/API/stats-api.md](docs/API/stats-api.md): packet reference and observed
  captures. Distinguish documented fields from behavior verified in a real capture.
- [docs/support.md](docs/support.md): diagnostics, privacy and troubleshooting.
- [docs/releasing.md](docs/releasing.md): release procedure.

Read the files relevant to the change rather than treating historical reviews as
a mandatory checklist. Verify review findings against current code; those reports
may describe issues that have already been fixed.

## Architecture and implementation

- Keep game integration out of process: local files and the loopback Stats API.
  Do not introduce injection, game-memory access or traffic interception as a
  routine feature implementation. Document deliberate changes to these boundaries.
- Keep Stats API parsing deterministic and separate from transport. Preserve
  match identity, result deduplication, replay exclusion and account-lock behavior.
- Keep network requests, database queries, package decoding and long filesystem
  operations off the UI render loop. Use bounded workers/queues and publish
  coherent snapshots through the existing state model.
- Preserve platform and `microsoft-store` feature gates. Linux checks do not prove
  Windows transparency, focus, passthrough or controller behavior works at runtime.
- Preserve checksum and signature verification for updates. Treat credentials,
  account identifiers, raw game logs and local paths as private in diagnostics.
- Game-file operations must retain backup verification, conflict detection and
  recoverability. Preview/read-only tools must not mutate source game files.
- Independently implement ideas from other projects; retain applicable licenses
  and attribution when intentionally reusing third-party code or assets.

## Validation

Use the pinned toolchain in `rust-toolchain.toml` and the committed `Cargo.lock`.
Use `--locked` for ordinary Cargo builds, checks and tests; update the lockfile
deliberately when changing dependencies.

During implementation, run focused checks appropriate to the change. Before
finishing substantive code changes, run `./check_all.sh` (or `check_all.bat` on
Windows). It checks formatting, Clippy with warnings denied, normal/all-feature
tests and the Microsoft Store feature configuration. Documentation-only edits
usually need link/content checks rather than another full build.

Add tests for meaningful behavior and failure cases. Keep installed-game tests
opt-in, follow their documented environment variables, and perform destructive
test operations only on temporary copies. Visually inspect material UI changes
using the existing headless review tools when practical.

Report what was actually verified. Identify skipped fixture tests and untested
platform behavior; do not present estimates or inferred performance improvements
as measurements.

## Git, documentation and releases

Honor the user's Git/publishing instructions and existing authorization. Preserve
unrelated edits and use an explicit lease when an authorized history rewrite is
necessary. Use commit messages that describe the actual change.

Update user/developer documentation when behavior changes. Add user-facing
release notes under `## [Unreleased]` in `CHANGELOG.md` when preparing a release.
Write all changelog entries in ASD-STE100 Simplified Technical English.
For a requested release, follow `docs/releasing.md` and use `release.sh`: it
requires a clean tree and can change versions, create commits/tags and push them.
Keep the dependency audit enabled; advisory handling is documented in
`docs/security-advisories.md`.
