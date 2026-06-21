## 2024-05-27 - Egui String Allocation Churn
**Learning:** In immediate-mode GUIs like egui, methods called every frame must strictly avoid heap allocations. Creating temporary `String` instances (e.g., using `.to_lowercase()` for string sorting or equality checks) causes massive allocator churn and severely degrades CPU performance.
**Action:** Always prefer zero-allocation string operations in hot paths, such as `.eq_ignore_ascii_case()` for comparisons, and `.bytes().map(|b| b.to_ascii_lowercase()).cmp(...)` for sorting instead of cloning to lowercase strings.
