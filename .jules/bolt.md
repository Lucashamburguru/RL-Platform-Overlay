## 2024-06-25 - Avoid String and Vec allocations in frequent parsing
**Learning:** In high-frequency UI paths (like parsing Rocket League stats API payloads every frame), allocating `Vec<&str>` via `.collect()` on `.split()` and creating new Strings with `.to_lowercase()` for simple matching creates noticeable overhead.
**Action:** Use iterator methods like `.next()` or `.nth()` directly instead of collecting to a `Vec`. Use `.eq_ignore_ascii_case()` on string slices for case-insensitive matching instead of allocating a new string with `.to_lowercase()`.
