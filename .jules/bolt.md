## 2024-06-20 - Eliminating String Allocations in High-Frequency Rendering Loops
**Learning:** Calling `.to_lowercase()` or `.to_ascii_lowercase()` on strings in rendering paths or sort comparators is a performance anti-pattern because it creates a new heap-allocated `String` for every check (e.g. 60+ times per second).
**Action:** Use `.eq_ignore_ascii_case()` for equality checks and `.bytes().map(|b| b.to_ascii_lowercase()).cmp(...)` (or `.chars()`) for sorting logic to perform case-insensitive comparisons without heap allocations.
