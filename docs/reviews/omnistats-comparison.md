# OmniStats comparison and takeaways

Review date: 2026-08-29  
OmniStats snapshot: [`accec7c2a303dbfe9f95530f5ac1c1b91c44d54f`](https://github.com/larrythemobster/OmniStats/tree/accec7c2a303dbfe9f95530f5ac1c1b91c44d54f) (`v2.1.3`, committed 2026-08-27)  
RL Platform Overlay snapshot: `2e3b1d03f11c9a442fc989efcfa949af788efd7e` plus the uncommitted working-tree changes present during this review

## Executive summary

OmniStats and RL Platform Overlay solve almost the same core problem: consume Rocket League's loopback Stats API, reduce noisy match events into useful state, enrich player data with Tracker Network, persist history, and render an out-of-process overlay/dashboard. They have independently made several similar product choices, but their engineering priorities differ.

OmniStats is a Windows-only C++ application with a mature match-history and statistics presentation, a configurable card/widget layout, explicit privacy controls, Discord presence, system-tray/startup integration, and extensive lifecycle-oriented integration tests. Its strongest architectural idea is the boundary between telemetry reduction and side effects: `TelemetryReducer` returns a `SideEffects` value and `SideEffectExecutor` performs database, network, Discord, and keypress work away from the parsing path.

RL Platform Overlay is a Rust application with Windows and Linux support, a more modern-looking second-monitor dashboard, WebSocket plus raw-TCP compatibility, teammate boost and touch-derived analytics, replay browsing/download/bulk upload, a Hoops replay fixer, a local cosmetic swapper, bounded diagnostic capture, and cryptographically signed update metadata. It also has a high volume of unit tests and benefits from Rust's ownership and type system. There is no reason to change language or rendering stack.

The most valuable lessons are therefore structural and product-level, not code to transplant:

1. Introduce a pure event-reducer result with explicit commands/side effects.
2. Create one coherent render snapshot per frame instead of loading many independently published values.
3. Grow history from player-encounter summaries into recent-match records and MMR timelines.
4. Generalize the dashboard and overlay layout around versioned widget placements.
5. Add stronger product privacy/data-management documentation before adding more integrations.

The OmniStats implementation must **not** be copied or adapted into this repository. OmniStats uses the PolyForm Internal Use License 1.0.0, which prohibits redistribution; this project describes itself as MIT. Treat the observations below as ideas and independently design any implementation.

## Side-by-side comparison

| Area | RL Platform Overlay | OmniStats | Assessment |
| --- | --- | --- | --- |
| Language and platform | Rust 2024; builds and tests on Linux and Windows | C++20; Windows 10+, MSVC, Win32/D3D11 | Keep Rust and cross-platform support. OmniStats' native Windows specialization is not a reason to narrow scope. |
| UI stack | `egui`/`eframe` | Dear ImGui + D3D11/Win32, ImPlot | Both are immediate-mode UIs. Our dashboard is visually clearer and more spacious; OmniStats exposes a more flexible widget model. |
| Game transport | Attempts WebSocket on `127.0.0.1:49123`, then detects and falls back to raw TCP with incremental JSON splitting | Raw TCP through Asio with a 5-second connect timeout, source-port/self-connection guards, 1 MiB buffer cap, and incremental JSON splitting | Our dual transport is broader. We should borrow the *defensive concepts* of explicit connect timeout and bounded receive buffering if equivalent protections are not already guaranteed by dependencies. |
| Telemetry pipeline | Pure parsing in `stats_api_parser.rs`, but event routing, shared-state mutation, persistence triggers, automation, and spawned work are coordinated in `network.rs` | `StatsClient` parses envelopes, `TelemetryReducer` updates state and emits `SideEffects`, and `SideEffectExecutor` runs external work | OmniStats has the clearer test seam. This is the highest-value architectural takeaway. |
| Shared/render state | Fine-grained `ArcSwap`, atomics, and locks grouped under `AppState`; UI panels load several values independently | Mutex-protected game/history models with version counters; UI creates a `RenderSnapshot` when versions change | Our reads are cheap, but independent loads can represent different publication instants. A frame-level snapshot would improve consistency and simplify render APIs. |
| Match lifecycle | Strong typed session/mode/result model, early-leave handling, replay discrimination, roster and score signatures | Detailed captured-match model, explicit finalize sources, void reasons, pending destroyed-match confirmation, finalized GUID set, reconnect safeguards | OmniStats' explicit lifecycle vocabulary and adversarial test matrix are worth emulating, especially for ambiguous `MatchDestroyed`/disconnect/forfeit sequences. |
| History | SQLite encounter history focused on games/wins/losses with or against players; history can be disabled | SQLite plus JSONL; recent matches, per-mode totals/streaks, encounter records, lifetime MMR history, MMR graph, export/delete actions | Recent-match history and MMR time series are the clearest missing user-facing capabilities in our project. JSONL duplication is optional and adds consistency complexity. |
| Live metrics | Roster score/goals/assists/saves/shots/touches/car touches/demos/boost, teammate boost, team comparison, event feed, estimated possession, teammate-bump estimation | Goals/assists/saves/shots/demos, goal participation, boost pickups, crossbars, fastest goal, ball/goal speed, impact force, own goals, MMR delta | Each has unique telemetry. OmniStats' session-level rich metrics could complement, not replace, our stronger team/possession dashboard. |
| Rank/MMR | Tracker lookup, per-playlist rank icons and match counts, local MMR refresh | Tracker lookup via an isolated compatibility runtime, bounded worker queue, per-playlist selection, pre/post-match MMR tracking and graph | MMR deltas and history are useful. Avoid taking on OmniStats' curl-impersonate repair/runtime complexity unless Tracker access makes it unavoidable. |
| Replay features | Ballchasing upload/download, cloud metadata caching, bulk upload with pause/stop/progress, uploaded-file tracking, Hoops replay repair | Ballchasing auto-upload and optional simulated save-replay keypress | Our replay toolset is materially broader. Auto-save is a small product idea, but simulated input and focus checks need careful UX and testing. |
| Desktop integration | Overlay/settings hotkeys, controller input, monitor selection, MSIX validation | Tray icon, run-on-startup, Discord Rich Presence, separate updater/MSI | Tray/startup and Discord are reasonable optional enhancements. They should not outrank correctness/history work. |
| Updates | GitHub releases; SHA-256 plus Ed25519 signature verification; release assets signed in CI | HTTPS download and published SHA-256 verification; separate updater process | Our authenticity model is stronger. Preserve it. A separate updater can improve replacement reliability on Windows, but also expands complexity and attack surface. |
| Privacy and diagnostics | Bounded in-memory Stats API log, explicit warning before exporting identifiable data, opt-in debug logging; no startup telemetry found | Versioned privacy/terms consent, required pseudonymous startup diagnostics, optional crash dump upload, documented data export/deletion | Adopt the documentation and local-data-control discipline, not required telemetry. Required startup diagnostics would be a privacy and trust regression for this project without a compelling service need. |
| Testing | About 30,039 lines under `src`, 259 `#[test]`/`#[tokio::test]` cases (mostly colocated), one 133-line external integration test; Linux and Windows CI | About 20,979 source lines and 7,173 test lines across 230 GoogleTest cases; dedicated pipeline, UI/headless, concurrency, database, updater, MMR, config, and lifecycle suites; optional benchmarks/fuzzer | Raw counts are not directly comparable. Our unit coverage is broad; OmniStats has the stronger dedicated integration-test organization and especially deep match-finalization matrix. |
| CI/security | Format, Clippy with warnings denied, all tests/features, Linux and Windows, MSIX validation, scheduled RustSec audit, pinned action SHAs | Windows build/test, clang-format, Gitleaks, Dependabot for actions | Our build/release verification is stronger overall. Add secret scanning and a PR template; consider expanding Dependabot coverage where useful. |
| Packaging/license | MSIX validation and standalone GitHub release binaries; README says MIT, but no `LICENSE` file was present | WiX MSI and separate updater; PolyForm Internal Use 1.0.0 plus narrow contribution exception | Add an actual MIT `LICENSE` file. Do not reuse OmniStats source, assets, branding, endpoints, or implementation-specific text. |

## What OmniStats does particularly well

### 1. Telemetry reducer and explicit side effects

[`TelemetryReducer.hpp`](https://github.com/larrythemobster/OmniStats/blob/accec7c2a303dbfe9f95530f5ac1c1b91c44d54f/src/core/TelemetryReducer.hpp) treats event handling as a state transition that returns a [`SideEffects`](https://github.com/larrythemobster/OmniStats/blob/accec7c2a303dbfe9f95530f5ac1c1b91c44d54f/src/core/SideEffects.hpp) description. Database saves, MMR refreshes, Discord updates, replay actions, and keypress simulation are then dispatched by `SideEffectExecutor`.

Our parsing layer is already usefully separated in [`src/stats_api_parser.rs`](../../src/stats_api_parser.rs), but [`src/network.rs`](../../src/network.rs) still combines transport, lifecycle routing, state publication, persistence triggers, delayed automation, and replay-upload triggers. A Rust-native equivalent could look conceptually like:

```text
raw bytes -> framed JSON -> parsed event -> reduce(old model, event)
                                      -> { new model, commands[] }
commands[] -> async executors (history, MMR, replay, automation)
```

This would make event-sequence fixtures deterministic, keep Tokio concerns out of match rules, and make duplicate/void/finalization behavior easier to reason about. The exact types and code should be designed independently.

### 2. Match-finalization rigor

OmniStats names multiple finalization sources and keeps a captured match, finalized GUIDs, pending destroyed matches, and explicit void reasons. Its largest test file is the match-validation suite, with 65 test cases and roughly 2,500 lines. That emphasis is justified: Stats API event order, reconnects, replays, forfeits, and partial lobbies are where silent history corruption happens.

Our typed `SessionMode`, `SessionModeSource`, `MatchResult`, signatures, and early-leave handling are a strong base. The useful next step is a table-driven event-sequence suite that asserts both final state and emitted commands for:

- normal `MatchEnded`;
- early leave and explicit forfeit;
- `MatchDestroyed` before or without `MatchEnded`;
- disconnect/reconnect during a match;
- replay playback events mixed with live events;
- missing, duplicated, or late match GUIDs;
- spectators and incomplete lobbies;
- duplicate terminal events and app restarts.

### 3. Versioned, configurable widget layouts

OmniStats represents dashboard content as widget IDs placed into zones and overlay content as widgets inside movable containers. Both layouts carry schema versions and are sanitized when loaded. That is more scalable than adding one boolean/position pair per new panel.

Our dashboard is already visually stronger: it has clearer team hierarchy, richer live tables, an event rail, and effective team-comparison graphics. The opportunity is to preserve that visual language while making cards reorderable/collapsible and allowing users to hide or relocate sections. A small first version could support a fixed registry of existing cards, persisted order/visibility, reset-to-default, and schema migration. Free-form containers can wait until a real user need appears.

### 4. Match history and MMR as a narrative

OmniStats makes saved games and MMR change first-class dashboard widgets. This gives the app value between matches, not only while a lobby is active. Our current history is more encounter-centric; it answers “have I played with this person?” well but does not yet offer a compact session diary.

The most useful additions would be:

- a normalized `matches` record with playlist/mode, result, score, timestamps, local stats, and optional pre/post MMR;
- a recent-games card with filters and clear void/incomplete handling;
- per-playlist MMR points linked to the match record;
- a simple MMR sparkline/graph;
- explicit export and delete-history controls.

This should build on the existing SQLite migration/recovery code in [`src/history.rs`](../../src/history.rs). Adding JSONL as a second source of truth is not necessary unless append-only recovery or external tooling becomes a concrete requirement.

### 5. Privacy and repository hygiene

OmniStats clearly inventories network destinations, triggers, fields, defaults, deletion behavior, and update trust boundaries in its [privacy](https://github.com/larrythemobster/OmniStats/blob/accec7c2a303dbfe9f95530f5ac1c1b91c44d54f/docs/PRIVACY.md) and [architecture](https://github.com/larrythemobster/OmniStats/blob/accec7c2a303dbfe9f95530f5ac1c1b91c44d54f/docs/ARCHITECTURE.md) documents. Its issue forms and PR template also warn contributors not to publish tokens, paths, logs, databases, or player data.

Our bounded diagnostic capture and save-time privacy warning are good. We should add a concise privacy/data-flow document covering Tracker, Ballchasing, GitHub update checks, local replay/config/history storage, exported logs, retention, and deletion. A Gitleaks workflow and public-report safety checklist are inexpensive improvements.

## Where RL Platform Overlay is already ahead

- **Transport compatibility:** WebSocket-first with raw-TCP fallback handles more Stats API configurations than OmniStats' TCP-only client.
- **Replay workflow:** download, metadata cache, bulk upload controls, local uploaded tracking, and Hoops replay repair go well beyond simple auto-upload.
- **Live dashboard design:** team-separated tables, boost, event feed, estimated possession, shot share, touch/car-touch metrics, and team comparison form a clearer at-a-glance match view.
- **Cross-platform engineering:** Linux development/testing and Windows/MSIX validation avoid a Windows-only architecture.
- **Update authenticity:** Ed25519 signatures in addition to checksums provide authenticity that checksum-only distribution cannot provide by itself.
- **Rust safety and checks:** Clippy with warnings denied, formatting, locked dependencies, all-feature tests, and scheduled RustSec audits are a strong baseline.
- **Support diagnostics:** the bounded two-minute in-memory event sample is privacy-conscious and purpose-built for parser/mode/team bugs.
- **Local utilities:** teammate boost display, bump estimation, Gold Rush swap, automation options, and layout dragging differentiate the product.

## Recommendations, in priority order

### P0: documentation and low-cost safeguards

1. Correct [`docs/architecture.md`](../architecture.md): it says telemetry arrives “via a BakkesMod plugin,” while current setup and networking use Rocket League's local Stats API directly. Update its module map as well; it omits the newer history, replay, diagnostics, dashboard, update-signing, and setup systems.
2. Add a root `LICENSE` file containing the MIT license if MIT is indeed the intended project license. A README statement alone is ambiguous for redistributors and automated tooling.
3. Add `docs/privacy.md` with an external-request/data-retention table and user deletion instructions.
4. Add Gitleaks (or equivalent secret scanning) and a PR template with test/privacy checkboxes.

### P1: correctness architecture

5. Define a pure `reduce_event` boundary that returns the next match/session model plus typed commands. Move persistence, MMR refresh, automation, and replay upload scheduling behind command handlers incrementally.
6. Build table-driven lifecycle integration fixtures around terminal-event ambiguity, duplicates, reconnects, replays, spectators, and incomplete matches.
7. Add explicit buffer/time limits to every transport path where the library does not already provide them, and test oversized/incomplete JSON plus stalled connection behavior.
8. Assemble one immutable UI frame snapshot (or a small number of domain snapshots with shared revision semantics) before rendering panels. This reduces cross-field tearing and long parameter lists.

### P2: durable user value

9. Add normalized recent-match history and an MMR-point table through versioned SQLite migrations.
10. Add recent-games and MMR-trend cards to the existing dashboard, keeping the current visual style.
11. Add local data export and a clearly scoped delete-history action. Keep config/secrets deletion separate so the button's effect is unsurprising.
12. Introduce a versioned dashboard-card registry with persisted order, visibility, collapsed state, and reset. Start constrained rather than implementing arbitrary containers immediately.

### P3: optional integrations and polish

13. Consider system tray and run-on-startup support on Windows.
14. Consider opt-in Discord Rich Presence only after a privacy/data-flow review.
15. Consider opt-in auto-save replay with a strict Rocket League focus check, configurable binding, debounce, visible status, and deterministic tests.
16. Consider richer event-derived session stats such as goal participation, speed records, crossbars, and own goals after verifying field stability against the project's Stats API documentation and real captures.

## Approaches not worth adopting directly

- **Do not port OmniStats code.** The license does not permit redistribution, and the contribution exception only permits changes proposed back to OmniStats.
- **Do not switch to C++/D3D11/ImGui.** It would sacrifice Rust safety and Linux support for no demonstrated product benefit.
- **Do not add required startup telemetry.** OmniStats documents it, but it creates infrastructure, consent, retention, availability, and trust obligations that this project does not need.
- **Do not adopt curl impersonation preemptively.** It is a brittle compatibility subsystem with extra binary repair and checksum responsibilities. Continue with the simpler Tracker path while it works and fails safely.
- **Do not duplicate SQLite history into JSONL without a recovery requirement.** Two persistence paths create ordering, migration, privacy, deletion, and consistency work.
- **Do not replace signed update verification with checksum-only verification.** Checksums detect mismatch; signatures establish publisher-controlled authenticity when the public key is safely embedded.
- **Do not copy OmniStats' dense visual styling.** Its configurability and information selection are useful, while our dashboard's hierarchy and legibility are stronger.

## Review limits

This was a static source, documentation, CI, and screenshot comparison. OmniStats was cloned shallowly into `/tmp` and was not built or run because its supported build requires Windows, Visual Studio, D3D11, and vcpkg. Runtime behavior and performance claims were therefore not independently measured. Approximate test and line counts are descriptive only: this project's Rust tests are mostly colocated under `src`, while OmniStats keeps most tests in a separate tree, so the numbers do not represent comparable coverage percentages.

No OmniStats source or assets were copied into this repository. This document records high-level observations and clean-room implementation ideas only.
