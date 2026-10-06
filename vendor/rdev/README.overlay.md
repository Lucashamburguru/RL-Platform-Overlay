# Local Windows listener fix

This directory contains rdev 0.5.3 from crates.io, under its original MIT license.
The app uses it through `[patch.crates-io]` until an upstream release includes
the fix.

Changes from 0.5.3:

- `src/windows/listen.rs`: provide a real `MSG` buffer to `GetMessageA`, process
  ordinary posted messages in a loop, exit on `WM_QUIT`, and report `-1` failures.
- `src/rdev.rs`: add `ListenError::MessageLoopError` for those Win32 failures.

The original listener passed a null output pointer and called `GetMessageA`
only once. It could run until a queued message was retrieved, then fail or stop
listening. This is a plausible delayed Windows failure, not confirmation of the
reported user's crash.

`tests/windows_listener.rs` in the app verifies that a benign posted message
does not stop the listener and that `WM_QUIT` exits it normally.

Upstream: https://github.com/Narsil/rdev
API contract: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmessage
