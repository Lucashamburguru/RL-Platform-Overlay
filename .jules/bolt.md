## 2024-10-18 - Avoiding String Allocations in egui Render Loops
**Learning:** In immediate-mode UIs like `egui` used in this Rust application, string operations like `.to_lowercase()` create heap allocations during every single frame (e.g. 60+ times per second). When used in sort closures or filtering loops inside the render path, this creates unnecessary memory pressure and CPU overhead.
**Action:** Always prefer zero-allocation string comparison methods in high-frequency paths. For equality, use `str::eq_ignore_ascii_case`. For case-insensitive sorting, use `str::bytes().map(|b| b.to_ascii_lowercase()).cmp(...)` to avoid allocating new `String` instances.

## 2026-06-17 - Avoid Unnecessary Clones for serde_json::Value
**Learning:** During JSON processing, particularly extracting the payload of an envelope structure, dereferencing and cloning an entire JSON `Value` object recursively causes deep memory duplication which severely blocks the main thread.
**Action:** Implemented a new enum variant extraction method `.into_owned()` that consumes the enum variants. When the string contains an encoded JSON, parsing creates an `Owned(Value)` which we can extract without cloning, only cloning `Borrowed` variants when absolutely necessary. This optimizes hot-path stats parsing when `decode_json_string_value` is called.

## 2024-10-18 - Zero-Allocation Substring Search in Render Loops
**Learning:** Checking for substrings inside immediate-mode render loops using `.to_lowercase().contains(...)` creates a large amount of heap allocations every frame, slowing down the UI thread and putting pressure on the memory allocator. This was happening repeatedly in playlist sorting and rendering operations in the `rl-platform-overlay` app.
**Action:** Implemented a new zero-allocation method `contains_ignore_ascii_case` using `.as_bytes().windows(len).any(...)` with `.eq_ignore_ascii_case(...)`. Always use this helper (added to `src/ui/common.rs`) for case-insensitive `contains` checks inside `egui` functions. Handled empty needles correctly to avoid panic.
