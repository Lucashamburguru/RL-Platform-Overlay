## 2024-10-18 - Avoiding String Allocations in egui Render Loops
**Learning:** In immediate-mode UIs like `egui` used in this Rust application, string operations like `.to_lowercase()` create heap allocations during every single frame (e.g. 60+ times per second). When used in sort closures or filtering loops inside the render path, this creates unnecessary memory pressure and CPU overhead.
**Action:** Always prefer zero-allocation string comparison methods in high-frequency paths. For equality, use `str::eq_ignore_ascii_case`. For case-insensitive sorting, use `str::bytes().map(|b| b.to_ascii_lowercase()).cmp(...)` to avoid allocating new `String` instances.

## 2026-06-17 - Avoid Unnecessary Clones for serde_json::Value
**Learning:** During JSON processing, particularly extracting the payload of an envelope structure, dereferencing and cloning an entire JSON `Value` object recursively causes deep memory duplication which severely blocks the main thread.
**Action:** Implemented a new enum variant extraction method `.into_owned()` that consumes the enum variants. When the string contains an encoded JSON, parsing creates an `Owned(Value)` which we can extract without cloning, only cloning `Borrowed` variants when absolutely necessary. This optimizes hot-path stats parsing when `decode_json_string_value` is called.
## 2024-07-04 - Zero-Allocation Substring Matching in Rust
**Learning:** `egui` immediate-mode UI renders at 60+ FPS. Using `.to_lowercase()` creates a new heap-allocated `String` every frame, causing performance regressions and stuttering. We can use `.as_bytes().windows(len).any(|w| w.eq_ignore_ascii_case(...))` for zero-allocation substring matching.
**Action:** Always prefer zero-allocation patterns in UI render loops. When checking if a string contains another string (case-insensitive), use `.as_bytes().windows()` and explicit `needle.is_empty()` checks to prevent `slice::windows(0)` panics, rather than allocating strings with `to_lowercase()`.
