# RL Platform Overlay — Master Review

**Compiled:** 2026-08-26
**Last updated:** 2026-08-29
**Baseline:** `v0.1.48` release candidate, including R6, R8, and R9
**Scope:** Consolidation of the repository's code, product, dashboard, UI/UX, Stats API, and performance reviews.

## Purpose and status rules

This document is the current review index and prioritized backlog. It replaces the need to read every historical review before choosing the next project, but it does not delete those reports or their detailed evidence.

When reports disagree, this review gives priority to the newest source and then checks the `v0.1.45` implementation. Statuses mean:

- **Complete** — implemented and covered by the normal repository checks or focused regression tests.
- **Partial** — the highest-value portion is implemented, but useful follow-up remains.
- **Open** — current code still shows the reviewed behavior.
- **Accepted** — intentionally retained behavior or risk, not an accidental omission.
- **Revalidate** — historical evidence may no longer match the dependency/runtime baseline and needs a fresh targeted check.

## Executive assessment

The application has a strong foundation. It is out-of-process, defensively parses the local Stats API stream, uses bounded caches and parser limits, verifies updater assets, has broad regression coverage, and keeps most expensive database and replay work away from the UI thread. The `v0.1.45` release also closes the most visible setup, layout, updater, and replay-list performance problems identified in the latest reviews.

No historical review found a critical issue. The most important remaining work is concentrated in four areas:

1. replace display-name identity keys with stable account keys;
2. reduce credential/privacy exposure and keep dependency policy release-gated;
3. finish responsiveness, accessibility, and the remaining measured performance work.

The original `v0.1.45` tag exposed two CI-only issues: a malformed updater mock-release fixture and new Rust 1.98 diagnostics promoted to errors by `-D warnings`. Both were corrected for the `v0.1.46` recovery release and validated locally on Rust 1.98 with formatting, Clippy, all targets, Microsoft Store features, unit tests, and integration tests. GitHub reported one low-severity Dependabot item during the original push.

## What has been completed

### Reliability and data handling

| Area | Result | Status |
|---|---|---|
| Configuration persistence | Shared mutations are serialized, stale writes are avoided, and persistence uses an atomic replacement path with concurrency coverage. | Complete |
| Stats API parsing | Buffer limits, numeric bounds, envelope variants, roster signatures, local-player fallbacks, rematch deduplication, and reset handling are covered. | Complete |
| Replay header safety | Oversized, truncated, deeply nested, and excessive metadata is rejected before proportional allocation. | Complete |
| Replay metadata concurrency | Local and cloud data use separate snapshots and a stable merged snapshot that updates only when a source changes. | Complete |
| Replay download writes | Staged writes, validation, quarantine, synchronization, and atomic replacement address partial final files. | Complete |
| Cloud pagination origin | Pagination remains scoped to the expected Ballchasing origin instead of forwarding credentials to arbitrary URLs. | Complete |
| HUD hotkey ownership | Cross-source keyboard handling is deduplicated and the Arrange HUD workflow has explicit lifecycle behavior. | Complete |
| MMR/cache bounds | Tracker-related caches are bounded and manual refreshes have a cooldown. | Complete |

### UI and updater

| Area | Result | Status |
|---|---|---|
| Setup readiness | Setup now shows installation, configuration, restart, process/connection, and recent packet readiness. | Complete |
| Launch readiness | Every overlay launch path rechecks `DefaultStatsAPI.ini` and routes an unready user to Setup. | Complete |
| Arrange HUD | Dedicated Arrange, Done, Cancel, and Reset All controls, reversible snapshots, and an overlay instruction banner replace the old drag toggle. | Partial — presets, snapping, guides, and keyboard nudging remain optional follow-up. |
| Dashboard labels | The cumulative Event Feed is now accurately named **Match Highlights**. | Complete |
| Dashboard header | Wide, medium, and compact arrangements prevent long status content from pushing the rest of the header away. | Complete |
| Team names | Stats API club/team names are displayed with Blue/Orange fallbacks; compact badges truncate Unicode safely and expose the full name on hover. | Complete |
| Updater notes | The updater displays the latest GitHub release body and the release workflow generates notes automatically. | Partial — notes are still plain text rather than rendered Markdown, and only the latest release is summarized. |

### Performance

| Area | Result | Status |
|---|---|---|
| Dashboard snapshots | Deferred dashboard rendering retains the shared `Arc<Config>` rather than deep-cloning replay-owned configuration. | Complete |
| Repaint cadence | Dashboard-only operation uses the 100 ms dashboard cadence; visible interactive HUDs keep the 16 ms cadence. | Complete |
| Replay list scaling | Merged metadata and derived rows are cached, and `TableBuilder` renders only visible replay rows. | Complete |
| Diagnostics | Production frame/foreground samples and explicit overlay CPU/RSS are recorded while performance capture is enabled. | Complete |
| History scaling | Database loading is backgrounded and history rows are virtualized. | Complete |

## Current prioritized backlog

### Priority 1 — Safety, integrity, and release policy

#### R1. Hoops fixer must not bless unrelated corruption — Complete (2026-08-27)

`fix_single_replay` still treats a CRC mismatch alone as a change worth rewriting, even when no recognized Hoops token was replaced. Require at least one recognized patch, reject invalid input CRCs, strictly parse input and output, and strengthen backup identity/versioning. Add bad-header CRC, bad-body CRC, marker-only, non-Hoops, and repeated-run tests.

**User impact:** Transparent hardening; normal successful repairs should behave the same.

**Resolution:** Repair now requires an exact recognized token replacement. Full
input and output parsing verifies both CRCs, marker-only and non-Hoops files are
left unchanged, repeated repair is a no-op, existing backups must match the
input byte-for-byte, and invalid backups cannot be restored.

#### R2. Strict replay validation on trust-changing paths — Complete (2026-08-27)

The bounded header parser is appropriate for fast library indexing, but a valid header does not prove the replay body and CRC are valid. Use full Boxcars validation with CRC checking before accepting downloads, suppressing a re-download, uploading, or replacing files. Keep rich network-frame parsing lazy.

**User impact:** Corrupt files will be rejected or quarantined instead of appearing successful.

**Resolution:** A bounded strict validator now parses the complete replay
container with Boxcars and always checks header/body CRCs without decoding
volatile network frames. Uploads, downloaded bytes, existing-file download
suppression, replacements, and Hoops mutations all use this boundary; replay
library scans retain their bounded header-only path.

#### R3. Stable player identity keys — Complete (2026-08-27)

Live players and several MMR paths are still keyed by display name (`HashMap<String, PlayerInfo>`). Duplicate names can collapse roster rows, and async results can attach to the wrong account. Introduce a normalized platform/account `PlayerKey`, with a match-scoped fallback for bots or temporarily unidentified players, and verify captured identity before publishing async results.

**User impact:** Correctness improvement; may require a state/cache migration but should not change intended workflows.

**Resolution:** Live player, roster, touch/debounce, replay-offset, and dashboard
maps now use a shared normalized `PlayerKey`. Account identities remain stable,
while bots and incomplete API identities receive match-scoped fallback keys.
Async player-MMR requests capture that key and publish only if the same identity
is still present. Regression tests cover duplicate display names, fallback scope,
and delayed same-name MMR results.

#### R4. Dependency advisory gate and release consistency — Complete (2026-08-27)

GitHub currently reports one low-severity dependency issue. Run a fresh advisory audit, update compatible transitive dependencies, document any time-bounded exception, and gate CI/releases consistently with a locked dependency check. The historical `quick-xml`/`eframe` findings must be revalidated against the current lockfile before planning a framework upgrade.

The existing `release.sh` also stages only `Cargo.toml` and pushes all local tags. Replace it with a release flow that updates both Cargo files and `CHANGELOG.md`, stages the intended release set, creates an annotated exact tag, pushes only that tag, and verifies the remote ref.

**Resolution:** Compatible `crossbeam-epoch` and `webbrowser` advisories were
updated. RustSec now gates releases and runs weekly, with the remaining
transitive `quick-xml` exceptions documented and expiring on 2026-10-01. Rust
1.98, action revisions, and lockfile use are pinned; local, CI, and release
checks share `check_all.sh`; the release helper owns the Cargo/changelog update
and exact annotated tag; and one final job publishes only after both platform
builds succeed.

### Priority 2 — Credentials, privacy, and concurrency correctness

#### R5. Protect the Ballchasing credential — Open

The token remains a serialized string in `config.toml`. As an immediate hardening step, enforce owner-only permissions on Unix and repair existing file permissions. Longer term, use the OS credential store and persist only a credential reference. Keep diagnostics limited to token presence.

#### R6. Redacted support bundles by default — Complete (2026-08-29)

Diagnostics can include user paths, player names, account identifiers, match IDs, and replay filenames. Default to a redacted bundle, allow identifiable details only through an explicit warning/choice, and show a preview of what will be copied.

**Resolution:** Support bundles now default to a redacted privacy mode that
removes local paths, player and account names, match and replay identifiers,
filenames, free-form errors, upload events, and recent debug logs while keeping
non-identifying state useful for troubleshooting. The dedicated Support tab
shows the exact clipboard preview, labels the redacted copy action, and requires
a session-only opt-in with a visible warning before identifiable details are
included. API keys remain excluded in both modes, and regression tests cover
the redaction boundary.

#### R7. Unify replay upload coordination — Open

Bulk and automatic uploads should use one coordinator keyed by canonical replay identity. Require multiple stable size/mtime samples plus strict validation before enqueueing, and deduplicate by replay ID or content hash where available.

#### R8. Make local MMR refresh atomic and identity-checked — Complete (2026-08-28)

The local refresh still has a check-then-set `fetching` transition and publishes completion without confirming the current local identity matches the captured request. Use an atomic request generation/coordinator and discard stale completions.

**Resolution:** Local MMR refreshes now claim a generation through one
mutex-backed coordinator, so concurrent requests for the same account collapse
to one operation. Account changes invalidate the active generation, clear the
old account's rank snapshot, and trigger a new refresh. Completion publishes
only while its captured generation and account remain current. Concurrent-claim
and superseded-account regression tests cover both race boundaries.

#### R9. Preserve unknown team state — Complete (2026-08-29)

Missing or invalid team data should not silently become Blue. Use `Option<u8>` or the existing no-team sentinel, accept only known team values for standard team logic, and preserve the prior team for the same stable player on partial frames.

**Resolution:** Player parsing now maps only teams 0 and 1 to standard teams
and uses the existing `NO_TEAM` sentinel otherwise. Stable `PlayerKey` matches
preserve the last known team across partial frames, including match-scoped bot
identities. Local-team, session-result, teammate HUD, dashboard, diagnostics,
and history-role boundaries reject unknown values; history stores them with an
`unknown` role rather than counting them as opponents. Regression tests cover
missing teams, out-of-range teams, fallback-key stability, partial live frames,
unknown match results, and history classification.

### Priority 3 — Responsiveness and measured performance

#### P1. Reduce packet-rate snapshot churn — Open, profile first

`UpdateState` still reconstructs and republishes several player, roster, diagnostics, and dashboard snapshots at packet rate. Record a sanitized event stream at 5/15/30/120 Hz, measure allocations and publications, then eliminate unchanged diagnostics and identity/roster publications without introducing fine-grained locking.

#### P2. Separate replay membership from preferences — Open

`uploaded_replays: Vec<String>` still lives in `Config`, combines recency and membership duties, and can make scans quadratic. Move it to a dedicated cache or SQLite table, maintain normalized set membership, batch persistence, and make unrelated settings edits independent of replay-library size.

#### P3. Move process enumeration off the UI thread — Open

Rocket League detection still calls `refresh_processes(ProcessesToUpdate::All, true)` synchronously. Publish the result from a low-frequency worker, request minimal process fields first, and measure Windows frame p95/p99 across scan boundaries. Preserve the borderless-style guard until runtime captures justify changing it.

#### P4. Stream and bound downloads — Open, lower urgency

Updater, replay, and asset downloads still buffer complete responses. Add conservative type-specific size limits and stream into uniquely named staged files while hashing. Preserve current signature guarantees.

#### P5. Bound Support diagnostics work — Open, lower urgency

The Support preview is now cached and rebuilds only on first display, a privacy
mode change, or an explicit refresh; Copy uses the displayed snapshot exactly.
This prevents repeated bundle allocation, hotkey-log reads, and Windows system
diagnostic probes on every repaint. Remaining work: read only a bounded tail of
the hotkey log, account for event strings and queue overhead in the recent API
log memory limit, add an explicit entry-count cap, and stream API-log exports
through a buffered writer instead of constructing a second full-size string.

### Priority 4 — UI/UX and product clarity

#### U1. Responsive settings and accessibility — Open

The app still forces `ctx.set_zoom_factor(1.0)`, uses a dense horizontal tab row, and relies on fixed widths/small secondary text. Respect platform scale or add an app scale setting, provide responsive navigation and one-column breakpoints, establish minimum text sizes, and test 100–200% scaling.

#### U2. Separate routine settings from advanced/risky tools — Open

Move Alpha Boost and repair/destructive replay tools behind a clearly labeled Advanced Tools/Utilities area. Split Replays into Upload, Library, and Repair sections and explain privacy/side effects before controls.

#### U3. Make persistence feedback explicit — Open

Show a stable “Changes save automatically” footer with brief Saved/Error feedback. Give filesystem operations specific labels such as “Write Stats API config.”

#### U4. Render updater notes safely — Open

Render a safe Markdown subset—headings, lists, emphasis, code, and links—and title it “What’s new in vX.Y.Z.” If only one release is fetched, say that the summary is for the latest version rather than all skipped versions.

#### U5. Use specific confirmation language — Open

The modal still says “Confirm Action” and “Yes, Proceed” for every operation. Use the actual verb/object, destructive styling, affected path/count, and safe keyboard focus order.

#### U6. Improve history retrieval and portability — Open

Add mode, teammate/opponent, result, and date filters, followed by CSV/JSON export and database backup. Keep Clear History visually separate from browsing/export actions.

#### U7. Validate contrast and non-color cues — Open

Measure semantic text/background pairs, define minimum contrast targets, and add high-contrast/color-vision-friendly options. Continue replacing local colors and font sizes with shared semantic tokens.

### Priority 5 — Maintainability and test depth

#### M1. Recorded Stats API fixture harness — Open

Replay sanitized 1v1, 2v2, 3v3, extra-mode, late-join, replay, and ready-up captures through the complete reducer. Store and report mode-detection provenance so incorrect classification is explainable.

**Progress (2026-08-29):** The normal receive pipeline now retains a bounded,
in-memory, one-second sample of `UpdateState` payloads plus all discrete events.
The Support tab can save the preceding two minutes with the app's detected
mode and provenance after a user notices a problem. The export is explicitly
marked identifiable, written only on request, and bounded to 8 MB. Turning
these reports into sanitized, committed reducer fixtures remains open.

#### M2. Break up large responsibility clusters — Open

Split replay transport/upload/download/index/validation and network transport/event projection/reduction behind narrow interfaces. Prefer pure transition functions with I/O in coordinators.

#### M3. Refresh architecture documentation — Open

Keep `docs/architecture.md` aligned with the direct Rocket League Stats API, current viewport/state model, updater verification, replay trust boundaries, and precise out-of-process safety wording. Avoid absolute anti-cheat guarantees.

#### M4. Harden Tracker integration — Open

Add saved-response contract fixtures, explicit freshness/stale indicators, persistent last-known-good values, and identity-checked publication for layout changes or request races.

## Recommended delivery order

1. **Integrity sprint:** R1 Hoops guard and R2 strict replay validation completed 2026-08-27.
2. **Identity sprint:** R3 stable `PlayerKey`, R8 stale-result protection, and R9 unknown-team handling.
3. **Release/privacy sprint:** R4 advisory/release gate and R6 diagnostics redaction are complete; R5 credential permissions/storage is deferred, so continue with R7 replay upload coordination.
4. **Measured performance sprint:** benchmark packet processing, then P1–P3; handle P4 alongside download hardening.
5. **Accessibility sprint:** U1, U3, U5, and U7 before broader settings reorganization.
6. **Product/maintenance sprint:** U2, U4, U6, M1–M4.

## Release and validation gates

Before the next release that changes a trust or identity boundary:

- formatting, Clippy, compile checks, unit/integration tests, and `git diff --check` pass;
- an advisory scan passes or each temporary exception has reachability reasoning and an expiry;
- replay mutations reject bad CRCs and preserve recoverable originals;
- async identity results are discarded when their captured account is no longer current;
- Windows CI covers the overlay and Microsoft Store feature set;
- UI changes are checked at 760, 900, and 1200 logical pixels and 100%, 125%, 150%, and 200% scale;
- performance claims include repeatable frame/CPU/RSS evidence rather than static inspection alone.

## Source review index

- [`2026-08-27-tech-debt-review.md`](2026-08-27-tech-debt-review.md) — current maintainability, ownership, testability, and release-reproducibility assessment.
- [`2026-08-26-performance-review.md`](2026-08-26-performance-review.md) — current performance evidence and implemented P1 work.
- [`2026-08-25-ui-ux-review.md`](2026-08-25-ui-ux-review.md) — current UI/UX heuristics and delivery phases.
- [`2026-08-20-full-project-code-review.md`](2026-08-20-full-project-code-review.md) — latest repository-wide safety, dependency, replay, identity, and maintainability review.
- [`2026-07-30-full-code-review.md`](2026-07-30-full-code-review.md) — earlier full audit and remediation history.
- [`2026-07-23-program-improvement-review.md`](2026-07-23-program-improvement-review.md) — product, fixture, CI, accessibility, and modularization recommendations.
- [`2026-07-15-code-review.md`](2026-07-15-code-review.md) and [`2026-07-09-code-review.md`](2026-07-09-code-review.md) — configuration, input, replay, credential, and pagination findings, many since resolved.
- [`2026-06-28-code-review.md`](2026-06-28-code-review.md) — Stats API conformance baseline.
- [`dashboard-review-2026-06-19.md`](dashboard-review-2026-06-19.md) — dashboard responsiveness and semantic-label review.
- [`2026-06-19-code-review.md`](2026-06-19-code-review.md), [`code-review-2026-06-17.md`](code-review-2026-06-17.md), and [`consolidated_review_2026-06-15.md`](consolidated_review_2026-06-15.md) — historical architecture and risk baselines; consult for rationale, not current status.

## Maintenance rule

Update this master review after each completed review item or repository-wide audit. Keep detailed evidence in dated source reviews, but reflect the current status and next priority here so resolved historical findings do not repeatedly re-enter the backlog.
