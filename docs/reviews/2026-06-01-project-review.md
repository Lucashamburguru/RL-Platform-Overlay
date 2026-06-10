# Project Review - 2026-06-01

## Scope

Reviewed the current Rust Rocket League overlay project after the teammate boost, debug capture, and update-checker changes. The pass covered runtime behavior, UI/settings flow, network parsing, input handling, configuration persistence, release workflow, dependencies, tests, and repo hygiene.

Commands run:

```text
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

`cargo test` passed with 7 tests. Strict clippy failed on style warnings promoted to errors; details are listed below.

## Findings

### High: Teammate boost HUD can still draw over the settings UI

Location: `src/ui.rs:54-68`, `src/ui.rs:1034-1037`

`show_settings` is calculated, but the floating teammate boost HUD renders whenever the overlay is launched and teammate boost is enabled:

```rust
if is_launched && config.show_teammate_boost {
    render_teammate_boost(ctx, &self.state);
}
```

That means opening settings while launched can still leave the live teammate boost panel in the foreground. This conflicts with the intended behavior from the recent UI work: settings mode should use the Boost tab preview instead of the in-game floating HUD. It also explains why the preview/boost overlay can feel like it blocks the GUI.

Recommended fix:

```rust
if is_launched && !show_settings && config.show_teammate_boost {
    render_teammate_boost(ctx, &self.state);
}
```

Keep the Boost-tab placement preview as the settings-mode visual. It already uses `egui::Order::Background`, so it is much less likely to intercept or visually cover controls.

### High: `Cargo.lock` is ignored and not tracked for a released binary app

Location: `.gitignore:3`, `Cargo.toml:1-24`, `.github/workflows/release.yml:60-61`

The repo ignores `Cargo.lock`, and `git ls-files Cargo.lock` returns nothing. For an application that publishes release binaries, this makes GitHub Actions builds non-reproducible. A new compatible dependency release can change the shipped binary without any source change.

This matters more now because the app depends on networking, windowing, input hooks, and `wreq`, all of which can change behavior through transitive updates.

Recommended fix:

1. Remove `Cargo.lock` from `.gitignore`.
2. Force-add the current lockfile once: `git add -f Cargo.lock`.
3. Keep it updated intentionally when dependencies change.

### Medium: Runtime `config.toml` is tracked and is mutated by normal app usage

Location: `config.toml`, `src/state.rs:72-89`, `.gitignore:13`

`config.toml` is tracked, but `Config::save()` writes to `config.toml` in the current working directory. The file is currently dirty from local settings changes:

```text
hotkey_ctrl = "LeftTrigger"
teammate_boost_offset = 420.0
teammate_boost_horizontal_offset = 1024.0
teammate_hud_scale = 2.5
teammate_boost_display = "Circles"
```

This creates two problems:

- Developers get a dirty working tree just by using the app.
- Release users may write settings beside the executable or whatever directory the process was launched from, and write failures are silently ignored.

Recommended fix:

Use a platform-specific user config directory, such as `directories`/`dirs`, and keep a sample config in the repo instead:

```text
config.example.toml
```

Then untrack the live config file:

```text
git rm --cached config.toml
```

### Medium: Config save failures are silently swallowed

Location: `src/state.rs:86-89`

`Config::save()` ignores filesystem errors:

```rust
let _ = fs::write("config.toml", content);
```

If the release binary is run from a protected location, or if the current working directory is unexpected, users can change settings in the GUI and then lose them on restart with no visible error.

Recommended fix:

Return `Result<()>` from `save()`, store the latest config error in `AppState`, and show it in the Debug tab or near the changed setting.

### Medium: Update checker has no explicit timeout

Location: `src/update.rs:23-28`

The update checker runs in a background task, so it should not block the render loop or the Rocket League data feed. However, the `wreq` request has no explicit timeout. A stalled DNS/TLS/connect/read path can leave the version check stuck for a long time, which leaves the Debug tab in a checking or failed-late state.

Recommended fix:

Configure a short client timeout, for example 3-5 seconds:

```rust
wreq::Client::builder()
    .timeout(std::time::Duration::from_secs(5))
    .build()?
```

If `wreq::Error` handling gets awkward after introducing `builder().build()?`, use a local error enum/string result so UI behavior stays simple.

### Medium: Release workflow does not run tests before publishing binaries

Location: `.github/workflows/release.yml:60-73`

The release workflow builds and uploads artifacts directly after setting up Rust. It does not run:

```text
cargo test
cargo clippy
```

The current local tests pass, but a tag can publish binaries even if tests fail in CI. Strict clippy currently fails locally, so it should either be fixed before adding clippy to CI or added without `-D warnings` at first.

Recommended fix:

Add at least `cargo test --all-targets` before `cargo build --release`. Add clippy once the current warnings are cleaned up.

### Low: Strict clippy currently fails on five warnings

Locations:

- `src/input.rs:127`
- `src/input.rs:159`
- `src/ui.rs:115-141`
- `src/ui.rs:672-679`
- `src/ui.rs:987-1000`

Local command:

```text
cargo clippy --all-targets --all-features -- -D warnings
```

Failures:

- Two `clippy::op_ref` warnings for comparing `&key_str[2..] == &config.hotkey_kb[3..]`.
- Two `clippy::collapsible_if` warnings in egui key-event handling.
- One `clippy::redundant_closure_call` warning in teammate team lookup.

These are not functional bugs, but they are quick cleanup and would unblock clippy in CI.

### Low: Update notice shows a URL but is not clickable

Location: `src/ui.rs:243-272`

The version-check banner tells users to download the newest release and prints the release URL, but it does not provide an action button or clickable link. Users can still manually copy the URL, but the flow is less polished than the rest of the settings UI.

Recommended fix:

Use `ui.hyperlink_to("Download release", &version_check.release_url)` or a small button that opens the URL through egui output.

### Low: GitHub tag lookup can point at tags that are not usable releases yet

Location: `src/update.rs:6`, `src/update.rs:23-67`

The update checker reads `/repos/.../tags`, finds the highest semver tag, and constructs a releases URL. This worked in the live test for `v0.1.5`, but tags and releases are not the same thing. If a tag is pushed and the release workflow fails, or if users open the app while the release is still building, the app may send them to a missing or incomplete release page.

Recommended fix:

Use the GitHub releases endpoint instead of tags when the goal is "download newest one":

```text
https://api.github.com/repos/Lucashamburguru/RL-Platform-Overlay/releases/latest
```

If prereleases should be ignored, check the `prerelease` and `draft` fields explicitly.

### Low: Raw TCP JSON splitting differs between app and debug tool

Locations: `src/network.rs:58-65`, `src/bin/debug_game_output.rs:200-207`

The debug tool only treats backslash as an escape while inside a JSON string:

```rust
'\\' if in_string => escaped = true,
```

The main network parser treats every backslash as an escape, even outside strings:

```rust
'\\' => escaped = true,
```

Real Stats API JSON is unlikely to contain bare backslashes outside strings, but the parser behavior should match between the app and debug tool to reduce confusion while investigating captures.

Recommended fix:

Change `src/network.rs` to use the same `('\\' if in_string)` condition as the debug utility.

### Low: README is stale after recent features

Location: `README.md:5-13`, `README.md:33-38`, `src/main.rs:31`

The README still describes only platform identification and says the graphics renderer is WGPU, while `src/main.rs` sets `eframe::Renderer::Glow`. It also does not mention teammate boost display modes, the update checker, or the debug capture binary.

Recommended fix:

Update the README after the next code cleanup so release users know:

- The app can show teammate boost.
- The update checker only notifies and does not auto-download.
- The debug tool can capture raw game output.
- The active renderer is Glow unless the renderer setting changes.

## Positive Notes

- The recent network parser tests cover the important local-player and bot-platform edge cases.
- The version comparison logic is unit-tested and the live GitHub test passed when temporarily compiled as `0.1.0`.
- The teammate boost display options are scoped to config and rendering code without spreading extra state across the app.
- The debug capture binary is useful and intentionally separate from the overlay runtime.

## Verification Results

`cargo test`:

```text
7 passed; 0 failed
```

`cargo clippy --all-targets --all-features -- -D warnings`:

```text
failed due to 5 clippy warnings promoted to errors
```

Current working tree before this review file:

```text
 M config.toml
?? rl_debug.txt
```

The review file is under `docs/`, which is ignored by the repo's current `.gitignore`.

## Suggested Fix Order

1. Hide the floating teammate boost HUD whenever settings are visible.
2. Track `Cargo.lock` for reproducible release builds.
3. Move live user config out of the repo/project root and stop tracking `config.toml`.
4. Add a timeout to the update checker.
5. Add `cargo test --all-targets` to the release workflow.
6. Clean the current clippy warnings, then consider adding clippy to CI.
7. Update README to match current features and renderer.
