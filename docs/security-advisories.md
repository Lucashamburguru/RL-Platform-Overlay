# Dependency advisory exceptions

The release and scheduled dependency audits fail on unacknowledged RustSec
advisories. No vulnerability exceptions are currently configured.

## Resolved `quick-xml` advisories

The GUI stack uses `eframe`, `egui`, and `egui_extras` 0.36.2. Its updated
accessibility dependencies no longer require `quick-xml` 0.30. Wayland protocol
generation uses `wayland-scanner` 0.31.11 and patched `quick-xml` 0.41.0.
The temporary RUSTSEC-2026-0194 and RUSTSEC-2026-0195 exceptions have been removed
from the release and scheduled audit workflows.

## HTTP dependency migration

The application now uses stable `wreq` 0.16.1 and `wreq-util` 0.2.0, replacing
the yanked 5.3.0 and 2.2.6 releases. Upstream reset the version numbering when
the 6.0 release-candidate line became stable 0.16. The migration also replaces
the unmaintained `lru` 0.13.0 dependency with 0.18.5.

System-proxy support, charset decoding, and gzip, Brotli, deflate, and Zstandard
response decoding are enabled explicitly to preserve the previous client
behavior. Browser profiles and redirect policies are preserved. Scheduled
audits continue to check the remaining dependency tree.
