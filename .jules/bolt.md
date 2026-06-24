## 2024-10-18 - Avoiding String Allocations in egui Render Loops
**Learning:** In immediate-mode UIs like `egui` used in this Rust application, string operations like `.to_lowercase()` create heap allocations during every single frame (e.g. 60+ times per second). When used in sort closures or filtering loops inside the render path, this creates unnecessary memory pressure and CPU overhead.
**Action:** Always prefer zero-allocation string comparison methods in high-frequency paths. For equality, use `str::eq_ignore_ascii_case`. For case-insensitive sorting, use `str::bytes().map(|b| b.to_ascii_lowercase()).cmp(...)` to avoid allocating new `String` instances.

## 2026-06-17 - Avoid Unnecessary Clones for serde_json::Value
**Learning:** During JSON processing, particularly extracting the payload of an envelope structure, dereferencing and cloning an entire JSON `Value` object recursively causes deep memory duplication which severely blocks the main thread.
**Action:** Implemented a new enum variant extraction method `.into_owned()` that consumes the enum variants. When the string contains an encoded JSON, parsing creates an `Owned(Value)` which we can extract without cloning, only cloning `Borrowed` variants when absolutely necessary. This optimizes hot-path stats parsing when `decode_json_string_value` is called.

## 2024-10-18 - Replacing allocating `to_lowercase().contains()` with zero-allocation alternatives
**Learning:** During egui immediate-mode render loops, string comparisons using `.to_lowercase().contains(...)` result in constant heap allocations, slowing down performance.
**Action:** Extract a shared `contains_ignore_ascii_case` utility in the codebase (e.g. `src/ui/common.rs`) that utilizes `haystack.as_bytes().windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle.as_bytes()))`. Additionally, replacing match statements over `.to_lowercase().as_str()` with if/else chains using `eq_ignore_ascii_case` further reduces memory pressure during frequent UI rendering frames.
