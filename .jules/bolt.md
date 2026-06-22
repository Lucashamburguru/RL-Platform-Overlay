## 2024-05-24 - High-Frequency Heap Allocations in Render Loops
**Learning:** Using `.to_lowercase()` on `String`s inside `egui` UI render loops or `sort_by` closures creates significant high-frequency heap allocations, which degrade performance.
**Action:** When performing case-insensitive string operations in hot paths (like every frame in a UI), use zero-allocation alternatives such as `.eq_ignore_ascii_case()` for equality checks, and `.bytes().map(|b| b.to_ascii_lowercase()).cmp(...)` for sorting comparisons.
