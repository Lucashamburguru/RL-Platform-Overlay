## 2024-10-18 - Avoiding String Allocations in egui Render Loops
**Learning:** In immediate-mode UIs like `egui` used in this Rust application, string operations like `.to_lowercase()` create heap allocations during every single frame (e.g. 60+ times per second). When used in sort closures or filtering loops inside the render path, this creates unnecessary memory pressure and CPU overhead.
**Action:** Always prefer zero-allocation string comparison methods in high-frequency paths. For equality, use `str::eq_ignore_ascii_case`. For case-insensitive sorting, use `str::bytes().map(|b| b.to_ascii_lowercase()).cmp(...)` to avoid allocating new `String` instances.

## 2026-06-17 - Avoid Unnecessary Clones for serde_json::Value
**Learning:** During JSON processing, particularly extracting the payload of an envelope structure, dereferencing and cloning an entire JSON `Value` object recursively causes deep memory duplication which severely blocks the main thread.
**Action:** Implemented a new enum variant extraction method `.into_owned()` that consumes the enum variants. When the string contains an encoded JSON, parsing creates an `Owned(Value)` which we can extract without cloning, only cloning `Borrowed` variants when absolutely necessary. This optimizes hot-path stats parsing when `decode_json_string_value` is called.
## 2024-11-20 - Avoid Unnecessary to_lowercase when checking strings

**Learning:** During UI rendering for immediate-mode UIs like `egui` (e.g. `src/ui/lobby_overlay.rs` and `src/ui/mmr_panel.rs`), performing string operations like `.to_lowercase()` or `.to_string().to_lowercase()` inside high-frequency render functions or utility functions leads to continuous heap allocation.
**Action:** Replace `.to_lowercase()` followed by `.contains()` or exact matches with `eq_ignore_ascii_case()` for exact matches or checking substring match with `.as_bytes().windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))`. Handled empty string explicitly to avoid panics. Ensure that you never use `to_lowercase` on strings inside render paths or data processing loops.
