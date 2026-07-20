## 2025-07-21 - [Avoid string allocations during case-insensitive directory scans]
**Learning:** During hot file-iteration loops (like `tokio::fs::read_dir`), using `.map(|s| s.to_lowercase()) == Some(target_filename.to_lowercase())` causes unnecessary O(N) double-string allocations.
**Action:** Use `.is_some_and(|s| s.eq_ignore_ascii_case(&target_filename))` to achieve a zero-allocation, case-insensitive comparison instead.
