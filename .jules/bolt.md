## 2024-06-25 - Avoid String Allocation During Case-Insensitive Filename Matching
**Learning:** Checking equality with `map(|s| s.to_lowercase()) == Some(target.to_lowercase())` allocates two new heap Strings on every iteration, leading to O(N) memory allocations during directory walks.
**Action:** Use `.is_some_and(|s| s.eq_ignore_ascii_case(&target_filename))` when scanning directories to eliminate all string allocations on case-insensitive comparisons.
