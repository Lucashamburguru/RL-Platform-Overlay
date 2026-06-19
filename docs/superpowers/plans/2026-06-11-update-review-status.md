# Update Review Status Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Mark every finding header in the original review files with ` [FIXED]` or ` [OPEN]` based on the master review summary.

**Architecture:** Systematic text replacement of headers in markdown files. Comparison against `docs/reviews/master_review_summary.md`.

**Tech Stack:** Text processing (grep/replace).

---

### Task 1: Update `docs/reviews/2026-06-01-project-review.md`

**Files:**
- Modify: `docs/reviews/2026-06-01-project-review.md`

- [ ] **Step 1: Mark finding headers**
    - "### High: Teammate boost HUD can still draw over the settings UI" -> "### High: Teammate boost HUD can still draw over the settings UI [FIXED]"
    - "### High: `Cargo.lock` is ignored and not tracked for a released binary app" -> "### High: `Cargo.lock` is ignored and not tracked for a released binary app [FIXED]"
    - "### Medium: Runtime `config.toml` is tracked and is mutated by normal app usage [FIXED]" -> Already has [FIXED], leave it.
    - "### Medium: Config save failures are silently swallowed" -> "### Medium: Config save failures are silently swallowed [FIXED]"
    - "### Medium: Update checker has no explicit timeout" -> "### Medium: Update checker has no explicit timeout [FIXED]"
    - "### Medium: Release workflow does not run tests before publishing binaries" -> "### Medium: Release workflow does not run tests before publishing binaries [FIXED]"
    - "### Low: Strict clippy currently fails on five warnings" -> "### Low: Strict clippy currently fails on five warnings [FIXED]"
    - "### Low: Update notice shows a URL but is not clickable" -> "### Low: Update notice shows a URL but is not clickable [FIXED]"
    - "### Low: GitHub tag lookup can point at tags that are not usable releases yet" -> "### Low: GitHub tag lookup can point at tags that are not usable releases yet [FIXED]"
    - "### Low: Raw TCP JSON splitting differs between app and debug tool" -> "### Low: Raw TCP JSON splitting differs between app and debug tool [FIXED]"
    - "### Low: README is stale after recent features [FIXED]" -> Already has [FIXED], leave it.

- [ ] **Step 2: Update Suggested Fix Order section**
    - Ensure all items in the list are marked as [FIXED].

### Task 2: Update `docs/reviews/code_review_2026_06_05.md`

**Files:**
- Modify: `docs/reviews/code_review_2026_06_05.md`

- [ ] **Step 1: Mark Architecture & Project Structure findings**
    - A1: "Split `ui.rs` monolith" -> [FIXED]
    - A2: "Glow vs `wgpu`" -> [FIXED]
    - A3: "Scratch files" -> [FIXED]
- [ ] **Step 2: Mark Correctness findings**
    - C4: "`Kp` → `Num` alias ASCII slicing" -> [OPEN]
    - C5: "`compare_versions` silences errors" -> [OPEN]
- [ ] **Step 3: Mark Concurrency & Thread Safety findings**
    - T1: "`save_config` store order" -> [FIXED]
    - T2: "Cached player path TOCTOU" -> [OPEN] (Master summary says TOCTOU in update_local_player_identity is fixed, but this T2 mentions players.load/mutate/store in mmr.rs which master summary says is still open/at risk)
    - T3: "`now_ms()` cast truncation" -> [FIXED] (Master summary says cleaned up timestamp usage)
    - T4: "`boost_swap_status` Mutex" -> [FIXED] (Master summary says verified all locks utilize safe patterns, but didn't explicitly say T4 is fixed by moving to ArcSwap. Wait, master summary section 6 says magic number 255 fixed. Section 1 says unsafe mutex unwraps fixed. Code Review 2026-06-05 says T4 is deferred. Let's check master summary again. Master summary doesn't explicitly mention T4. I'll mark it [OPEN] unless I see evidence.)
- [ ] **Step 4: Mark Error Handling findings**
    - E1-E4: All [FIXED] according to implementation update.
- [ ] **Step 5: Mark Performance findings**
    - P1: "Repaint optimization" -> [FIXED]
    - P2: "`is_rocket_league_running` allocs" -> [FIXED] (Section 3 says smart CoW and throttled updates fixed excessive cloning)
    - P3: "`local_player_name` allocs" -> [FIXED]
    - P4: "tracker.gg warmup" -> [FIXED]
- [ ] **Step 6: Mark Code Duplication findings**
    - D3: "Platform-matching logic" -> [OPEN] (Section 6 only mentions replay upload cache helper fixed)
- [ ] **Step 7: Mark Security & Safety findings**
    - S1: "Hard-coded User-Agent" -> [FIXED]
- [ ] **Step 8: Mark Testing findings**
    - TS1-TS4: All [FIXED] according to master summary.
- [ ] **Step 9: Mark CI/CD findings**
    - CI1-CI2: All [FIXED].
- [ ] **Step 10: Mark Documentation findings**
    - DOC1-DOC2: All [FIXED].

### Task 3: Update `docs/reviews/code_review_2026-06-10.md`

**Files:**
- Modify: `docs/reviews/code_review_2026-06-10.md`

- [ ] **Step 1: Mark Critical findings**
    - 1: Race condition -> [FIXED]
    - 2: `number_field` u64 -> [FIXED]
- [ ] **Step 2: Mark Moderate findings**
    - 3: HashMap iteration order -> [OPEN]
    - 4: Replay upload backpressure -> [FIXED]
    - 5: `players.rcu` clone -> [FIXED]
    - 6: Debug print -> [FIXED]
    - 7: Large log files -> [FIXED]
    - 8: Stale test files -> [FIXED]
    - 9: `auto_freeplay` hardcoded -> [FIXED]
    - 10: `wreq::Client` per-request -> [FIXED]
- [ ] **Step 3: Mark Minor findings**
    - 11-16: All [OPEN] unless explicitly fixed in master summary. (11, 12, 13, 14, 15, 16 seem mostly open/minor nits)

### Task 4: Update `docs/reviews/code-review-2026-06-10.md` (Note the hyphen/underscore difference)

**Files:**
- Modify: `docs/reviews/code-review-2026-06-10.md`

- [ ] **Step 1: Mark prioritized recommendations**
    - Identify headings from the end of the file or sections.
    - 1: Panic in parse_platform -> [FIXED]
    - 2: Guard Mutex unwraps -> [FIXED]
    - 3: Extract replay upload cache helper -> [FIXED]
    - 4: Use ArcSwap::rcu -> [FIXED] (Master summary says smart CoW check on RCU merging fixed it)
    - 5: Replace Vec with HashSet -> [FIXED]
    - 6: NO_TEAM const -> [FIXED]
    - 7-10: All [FIXED].

### Task 5: Update `docs/reviews/performance-review-2026-06-08.md`

**Files:**
- Modify: `docs/reviews/performance-review-2026-06-08.md`

- [ ] **Step 1: Mark Critical findings**
    - 1: Config saved on every drag -> [FIXED]
- [ ] **Step 2: Mark High findings**
    - 2-6: All [FIXED] according to master summary section 3.
- [ ] **Step 3: Mark Medium findings**
    - 7: Sync Config::save -> [FIXED]
    - 8: Vec remove(0) -> [FIXED]
    - 9: uploaded_replays Vec -> [FIXED]
    - 10: boost_operation_running Mutex -> [FIXED]
    - 11: Sync fs in async -> [FIXED]
    - 12: to_lowercase in sort -> [FIXED]
    - 13: Config clone in settings -> [FIXED]
    - 14: Log Vec clone -> [FIXED]
    - 15: local_player_name store -> [FIXED]
- [ ] **Step 4: Mark Low findings**
    - 16-21: Most [OPEN] (master summary doesn't mention them)

### Task 6: Update `docs/reviews/ux-review-2026-06-08.md`

**Files:**
- Modify: `docs/reviews/ux-review-2026-06-08.md`

- [ ] **Step 1: Mark Critical findings**
    - 1: Confirmation on destructive actions -> [FIXED]
    - 2: HUD status misleading -> [FIXED]
- [ ] **Step 2: Mark Tab Organization findings**
    - 3-5: All [FIXED] (Master summary section 4).
- [ ] **Step 3: Mark Settings UX findings**
    - 7-11: [FIXED].
    - 12-14: [FIXED] (Master summary says UX section 4 fixed UI organization/missing feedback).
- [ ] **Step 4: Mark Hotkey System findings**
    - 15-18: [FIXED] (Moved hotkeys section to Setup tab, likely polished behavior).
- [ ] **Step 5: Mark Overlay Rendering findings**
    - 19-21: [FIXED] (Relocated positioning controls directly to respective Lobby, Session, and Boost tabs).
- [ ] **Step 6: Mark Empty/Error State findings**
    - 22-24: [FIXED] (Surfaced config save failures, auto-detect errors).
- [ ] **Step 7: Mark Accessibility findings**
    - 25-27: [FIXED] (Added tooltips).
- [ ] **Step 8: Mark Inconsistencies findings**
    - 28-30: [FIXED].
