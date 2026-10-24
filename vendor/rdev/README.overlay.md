# Local input listener fixes

This directory contains rdev 0.5.3 from crates.io, under its original MIT license.
The app uses it through `[patch.crates-io]` until an upstream release includes
these fixes.

Changes from 0.5.3:

- `src/windows/listen.rs`: provide a real `MSG` buffer to `GetMessageA`, process
  ordinary posted messages in a loop, exit on `WM_QUIT`, and report `-1` failures.
- `src/rdev.rs`: add `ListenError::MessageLoopError` for those Win32 failures.
- `src/linux/listen.rs` and `src/linux/common.rs`: replace mutable global keyboard
  and callback state with state passed through XRecord's callback pointer. Keep
  the client selector local to the listener. Reject reentrant callback borrows.
  Decode the byte buffer without native alignment requirements or Rust boolean
  fields, and release recorded data for all notification categories. Release
  the record context and display connection when the listener returns.

The original listener passed a null output pointer and called `GetMessageA`
only once. It could run until a queued message was retrieved, then fail or stop
listening. This is a plausible delayed Windows failure, not confirmation of the
reported user's crash.

`tests/windows_listener.rs` in the app verifies that a benign posted message
does not stop the listener and that `WM_QUIT` exits it normally.

The Linux listener has hardware-free unit tests for independent callback state,
reentrant dispatch, wire offsets, and empty notifications. Run them with:

```sh
cargo test --manifest-path vendor/rdev/Cargo.toml --lib linux::listen::tests
```

The Linux state remains alive until the synchronous `XRecordEnableContext` call
returns. These tests do not replace an X11 desktop hotkey test.

Upstream: https://github.com/Narsil/rdev
API contract: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getmessage
XRecord contract: https://www.x.org/releases/X11R7.7/doc/libXtst/recordlib.html
