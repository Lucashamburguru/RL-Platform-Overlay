# Hebnix source comparison and improvement proposals

Reviewed on 2026-11-07. Hebnix was shallow-cloned to `/tmp/Hebnix-Public`, at
commit [`536e5bee50f85f8a91b8a85a18e66f658084d38a`](https://github.com/HebnixRL/Hebnix-Public/tree/536e5bee50f85f8a91b8a85a18e66f658084d38a),
workspace version 2.2.6. Our baseline is commit
`be5b361c3c2a40847e709e91bf58ccdb09c7c904`, version 0.1.64, including the
working tree visible during review. Existing user changes were left untouched.

This is a source-based product and architecture comparison. Neither application
was launched or benchmarked, and the feature suggestions below are proposals,
not implemented changes. Presence in source does not establish reliability in a
current Rocket League build or compatibility with every compositor.

## Overall recommendation

Hebnix is a broad Rocket League customization and plugin platform. We already
cover the central overlay workflows: lobby ranks, teammate boost, sessions,
encounter history, replay management, and item/boost sound replacement. Our best
opportunity is to make those workflows more accurate, visual, and convenient.

The most useful ideas to adapt are supplemental match context from game logs,
item previews and shareable presets, brief encounter notifications, richer match
events, and dedicated Linux integration. Preserve our independent telemetry
pipeline, verified file restoration, and signed updater while adding them.

## Feature comparison

| Area | Hebnix source | Our current implementation | Opportunity |
| --- | --- | --- | --- |
| Rank/platform overlay | Native overlay plus Lua rank/platform examples | Native lobby HUD, per-player details, playlist/peak ranks, team comparison dashboard | Improve context and freshness indicators; another rank HUD adds little |
| Repeat-player awareness | `heads_up` plugin stores encounters and emits threshold/MMR notices | SQLite encounter history, relationship summaries and player details | Add an optional short match-start notice using existing history |
| Match identification | Stats API plus `Launch.log` playlist, map, identity and offline context | Stats API metadata with mode evidence and inference | Add bounded log metadata as corroborating evidence |
| Item replacement | Item browser, local thumbnail extraction, recoloring, owned inventory filtering, presets | Searchable installed catalog; appearance and engine/boost audio swaps; per-item restoration | Previews and declarative presets are the best initial additions |
| Replay workflows | Auto-upload, event timeline, seek/speed/POV keyframes, additional history/recovery integration | Upload/download/cloud sync, content-keyed ledger, metadata, player scores, goal list, Hoops repair | Start with visual event navigation; evaluate replay control separately |
| Extensibility | Lua lifecycle/events/settings/windows/overlay drawing and example plugins | Built-in Rust panels and tools | Establish an internal widget interface before a public scripting host |
| Presentation | TOML themes, custom fonts, Fluent locale catalogs | Configurable lobby themes, HUD styles, scaling and arrangement | Named HUD profiles and shared semantic color tokens first |
| Linux | Wayland layer-shell and X11 overlays, Wine-prefix discovery, launcher integration, packaging | egui viewport path; replay-path candidates include common Steam/Flatpak prefixes | Dynamic prefix discovery and a compositor-specific overlay prototype |
| Convenience | Tray controls, media overlay examples, Discord presence | Global keyboard/controller controls and settings/dashboard windows | Tray controls are useful; presence/media widgets are optional later work |

Evidence: Hebnix [examples][examples], [log parser][log], [swapper][swapper],
[presets][presets], [timeline][timeline], [theme module][themes], [locales][locales],
[Linux overlay][wayland], and [prefix discovery][prefixes]. Our corresponding
code is in [dashboard](../src/ui/dashboard.rs), [history](../src/history.rs),
[parser](../src/stats_api_parser.rs), [item swapper](../src/item_swapper.rs),
[replay UI](../src/ui/settings/replays.rs), and [configuration](../src/state.rs).

## Recommended work, in order

### 1. Supplement match identity and playlist detection with Launch.log

**Value: high. Effort: medium.** Hebnix reads session identity, playlist IDs,
game class, map loads and offline indicators from the log. Its parser explicitly
avoids inheriting playlist/map information from an older match. This addresses
a real ambiguity in ours: player counts and arena names do not always establish
the playlist, especially with partial rosters and shared arenas.

Implement a separate background log reader and pure parser. Publish a compact
metadata snapshot with source, observed time and map-load generation. Keep the
Stats API authoritative for live scores, players, clocks and match lifecycle.
Only accept log context after associating it with the current game/match; discard
it on menu transitions, log rotation, truncation, process restart or expiry.
When association is uncertain, retain our existing inference and expose that
uncertainty in diagnostics.

Start with playlist ID and offline context; account identity can be an additional
hint rather than silently replacing an explicitly selected account. Read bounded
increments and handle platform paths off the UI thread. Export only allowlisted
metadata in support reports because logs can contain account/server data.

Change points: new log-context module, `stats_api_parser.rs`, `session.rs`,
`diagnostics.rs`, and shared snapshots in `state.rs`. This intentionally extends
the documented data-source model; update `architecture.md` alongside it.
Acceptance checks: stale previous-match playlist, private match after ranked,
menu transition, partial lobby, rotated/truncated log, absent log, and disconnect.

### 2. Add item thumbnails and shareable cosmetic presets

**Value: high. Effort: medium for preset definitions; larger for extraction.**
Hebnix extracts thumbnails in background workers and supports versioned encoded
preset definitions. Our selector is principally a text list, and our Gold Rush
preset is a special case rather than a general named preset library.

Deliver in two increments:

1. Save/load named swap definitions with a versioned schema, stable catalog IDs,
   explicit sound choices, and an import preview listing unavailable items and
   shared-bank conflicts. Apply through our existing mutation pipeline. Define
   all-or-nothing or explicit partial-apply behavior before implementing batch
   application; do not accidentally promise cross-item atomicity.
2. Add asynchronous, bounded thumbnail decoding/cache and a source/target preview.
   Use local package/version identity in cache keys and retain a working text
   fallback when a texture cannot be extracted. A thumbnail is not a rendered
   preview of the equipped car.

Share definitions only: exclude credentials, absolute paths, backups and game
package bytes. Separate HUD layout profiles from cosmetic presets so importing
presentation settings cannot modify game files. Preserve Microsoft Store feature
gating. Do not start with save editing or ownership emulation just to obtain an
owned-only filter.

Change points: `item_swapper.rs`, `upk_swap.rs`, `ui/settings/item_swapper.rs`,
and a new preset store. Acceptance checks: missing donor, catalog change,
conflicting shared sound bank, damaged thumbnail, old preset schema and a failed
batch apply.

### 3. Add an optional match-start encounter notice

**Value: medium/high. Effort: small/medium.** Hebnix's `heads_up` example waits
for the roster to settle and rank requests to finish or time out before showing
a brief notice about repeat encounters and MMR differences.

Use our existing SQLite summaries to show a compact notice such as
“Played with Alex 4 times · 3W–1L together,” with an optional team MMR comparison.
Make the trigger and duration configurable, deduplicate by match and player ID,
handle late joins without repeated notices, and tolerate missing ranks.

Our history measures completed encounters; Hebnix's plugin records appearances.
Keep that distinction explicit in the wording. Avoid describing a rank difference
as evidence of a smurf or predicting a win from incomplete cached ranks.

Change points: `history.rs`, `network.rs`, and a small HUD notification component.
Reuse the rank provider and history snapshots; no new service is needed.

### 4. Retain useful match events and improve replay review

**Value: high. Effort: medium for events; high for playback control.** Hebnix
models goal speed/scorer/assister, ball-hit speeds/location, and crossbar hits.
Our event router currently groups `BallHit`, `GoalScored`, and `StatfeedEvent`
mainly as evidence that a round started. We already extract and display replay
goals, so a goal list alone would duplicate existing functionality.

Add a bounded typed event timeline for verified live matches, then enrich the
completed-match dashboard with goal speed, scoring sequence, assists and demos
where the API actually supplies them. Associate events with match identity and
deduplicate replayed/repeated events. Missing values must stay unknown.

For replay files, visualize our existing goal frames on a timeline, add local
bookmarks, and link the history/replay views when identity is reliable. Label
bookmarks as local metadata. Add saved-match detail queries before attempting
longitudinal performance charts: our history schema is currently oriented toward
encounters and deliberately excludes local-player encounter rows.

Hebnix also sends Stats API commands for replay seek, speed, pause and POV.
That is a separate capability experiment: our transport is receive-oriented,
and commands need validation against our supported WebSocket/raw TCP modes and
current game builds. Gate any eventual controls to positively identified replay
playback; no command should reach a live competitive match.

Do not infer continuous heatmaps, true possession, boost-pad efficiency or full
car trajectories from isolated hit positions and scoreboard snapshots. Those
features require a confirmed richer data source, such as offline replay decoding.

### 5. Improve Linux discovery and evaluate native overlay surfaces

**Value: high for Linux users. Effort: medium for paths; large for rendering.**
Hebnix discovers Steam libraries, arbitrary Proton shortcut IDs, Heroic prefixes
and explicit custom prefixes. Our replay defaults enumerate common Steam paths
using app ID 252950; that misses many Epic/non-Steam/custom installations.

Build one bounded installation/prefix resolver shared by Setup, replays, logs
and local files. Offer candidate selection when several installations exist;
do not silently pick a different account's newest directory. Include Flatpak,
secondary libraries, Heroic and explicit custom roots in fixture coverage.

Separately prototype a wlr-layer-shell HUD with an empty input region and no
keyboard focus. Keep the dashboard/settings in egui. Layer-shell availability
must be detected; it is not universal Wayland support. Preserve a fallback and
test compositor behavior, monitor scaling, fullscreen and hotkeys independently.

Hebnix also has a Windows DirectComposition renderer with GDI fallback. This is
worth studying for our transparency/window issues, but a renderer rewrite needs
reproductions and measurements first. Its source comments are not proof of
performance gains or anti-cheat acceptance.

### 6. Establish widget boundaries before adding Lua plugins

**Value: medium now, potentially high later. Effort: large for public plugins.**
Hebnix demonstrates how small user extensions can subscribe to events, render
settings and draw overlays. Its manifest currently declares local read roots;
that alone should not be treated as a complete capability sandbox.

First define a versioned internal event/snapshot interface and a widget contract
for visibility, layout, rendering and configuration. Use it for notifications,
goal-speed display and existing HUD panels. Add named layout profiles and
export/import of allowlisted display settings.

If third-party scripting becomes a requirement, isolate execution from the UI,
bound memory/instructions/queues, explicitly grant file/network/capture/game
command access, and disable failing plugins independently. A marketplace,
arbitrary window capture and general HTTP scripting should not be prerequisites
for a customizable HUD.

## Engineering lessons and places we can do better

- **Keep event processing away from rendering.** Hebnix's `ui_work.rs` bounds
  UI-consumed message batches to 128 messages or 2 ms and prioritizes controls.
  Our Tokio processing plus immutable snapshots already avoids that particular
  UI queue. Preserve it. If we add thumbnail/plugin queues, bound the queue and
  per-frame drain rather than copying an unbounded channel.
- **Model window transitions explicitly.** Hebnix's visibility gate rejects
  toggle bursts while a transition settles and expires stalled transitions.
  Consider the pattern if we reproduce hotkey/window desynchronization. Our
  existing key edge handling and repaint tests remain useful; a gate alone
  does not fix every Windows border/transparency issue.
- **Measure rendering before replacing it.** Hebnix avoids presenting unchanged
  native overlay frames. We already target repaints to affected viewports and
  slow idle updates. Compare idle CPU/GPU, active 30 Hz telemetry, dashboard
  open, alt-tab recovery and game frame times on the same machine before
  claiming either implementation is faster.
- **Preserve per-file restoration checks.** Hebnix's backup guard invalidates
  old backups when the executable hash changes. Our swapper tracks original
  and applied hashes, validates package identity and coordinates shared audio
  ownership. Build identity could add context, but should not replace those
  per-file checks. Quarantining stale backups retains recovery evidence better
  than indiscriminately deleting them.
- **Preserve our signed updater.** Our Windows updater checks SHA-256 and an
  Ed25519 signature. Hebnix's reviewed `update.rs` fetches update metadata and
  a setup URL; this review does not establish equivalent verification elsewhere
  in its distribution pipeline. Keep our verification contract intact.
- **Reuse rank coordination, expose freshness.** Both projects already cache
  ranks. Hebnix's SDK default TTL is five minutes; our lobby cache TTL is one
  hour with separate local-player refresh logic. Add visible fetch age and
  deliberate refresh policy rather than copying a TTL or increasing requests
  blindly. Respect provider capacity and existing stale-result protection.
- **Keep modules focused.** Hebnix separates an SDK from its application, but
  its app and Lua host still have large modules. We can obtain the useful
  separation through typed domain interfaces without immediately splitting
  our repository into a workspace or moving everything into `AppState`.
- **Prepare localization incrementally.** Hebnix uses Fluent with English marked
  reviewed and four other shipped locales marked draft. Adopt message IDs,
  fallback and validation when translations are planned; file presence alone
  is not evidence of translation quality.

Sources for these lessons: [UI batch handling][ui-work], [visibility state][visibility],
[native renderer][native], [backup guard][backup], [update module][update],
[tracker cache][cache], and our [architecture](architecture.md),
[repaint signaling](../src/ui_repaint.rs), [MMR worker](../src/mmr.rs),
[swap pipeline](../src/item_swapper.rs), and [updater](../src/update.rs).

## Features to leave outside this roadmap

Hebnix has game-session proxy/spoofing components, process-memory token scanning
for replay recovery, save/loadout editing, workshop multiplayer orchestration and
other game-file patchers. These are materially different integrations, not
requirements for improving our overlay. In particular,
[replay recovery][replay-source] imports a [process-memory scanner][memory],
and [the spoofer][spoofer] includes proxy/CA integration. Carrying those into ours
would expand credentials, platform coupling and maintenance responsibilities and
conflict with our current game-access boundary.

Discord presence and media/window widgets are lower-priority conveniences. They
are feasible without making them dependencies of core match functionality.

## Source reuse

The repository carries a custom [Hebnix License][license] and [NOTICE][notice],
not our MIT license. Sections 2 and 4 describe reuse and redistribution terms,
including license delivery, modification notices and retained attribution.
Treat any copied implementation as a separately attributed dependency/source
component and check its applicable notices before distribution. The proposals
above can be independently implemented within our existing architecture; this
review imports no Hebnix code or assets.

[examples]: https://github.com/HebnixRL/Hebnix-Public/tree/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/examples/plugins
[log]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-sdk/src/log/parser.rs
[swapper]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/patcher/swapper.rs
[presets]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/presets.rs
[timeline]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/replay_timeline.rs
[themes]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/theme.rs
[locales]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/locales/locales.toml
[wayland]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/overlay/wayland.rs
[prefixes]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-sdk/src/process/wine_prefix.rs
[ui-work]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/ui_work.rs
[visibility]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/visibility.rs
[native]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/overlay/native.rs
[backup]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/patcher/backup_guard.rs
[update]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/update.rs
[cache]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-sdk/src/tracker/cache.rs
[replay-source]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/replay_source.rs
[memory]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/replay_process_memory.rs
[spoofer]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/hebnix_rs/crates/hebnix-app/src/spoofer/mod.rs
[license]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/LICENSE.md
[notice]: https://github.com/HebnixRL/Hebnix-Public/blob/536e5bee50f85f8a91b8a85a18e66f658084d38a/NOTICE.txt
