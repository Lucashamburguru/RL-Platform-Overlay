# Dependency advisory exceptions

The release and scheduled dependency audits fail on unacknowledged RustSec
advisories. The exceptions below are temporary, reviewed constraints rather than
blanket audit suppression.

## RUSTSEC-2026-0194 and RUSTSEC-2026-0195 (`quick-xml`)

- **Review deadline:** 2026-10-01
- **Affected transitive versions:** `quick-xml` 0.30 and 0.39
- **Constraint:** `eframe` 0.31's accessibility stack selects 0.30, while its
  Wayland code-generation stack selects 0.39.
- **Current exposure:** These copies are used by desktop accessibility and
  Wayland support. They are not the parsers for the application's Stats API,
  HTTP responses, or replay files, which substantially limits reachability from
  untrusted application data.
- **Removal plan:** Upgrade `eframe` and its platform stack when a compatible
  release removes both affected versions, then delete the workflow exceptions.

## Yanked and unmaintained transitive packages

`wreq` 5.3.0 is yanked and currently brings in an unmaintained `lru` release.
The next published `wreq` line is release-candidate software with breaking API
changes, so the application remains pinned to its reviewed lockfile until a
stable compatible upgrade is available. Scheduled audits keep new advisories
visible while that migration is pending.
