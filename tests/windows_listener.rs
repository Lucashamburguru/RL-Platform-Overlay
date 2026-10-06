#![cfg(windows)]

use std::sync::mpsc;
use std::time::{Duration, Instant};
use winapi::um::processthreadsapi::GetCurrentThreadId;
use winapi::um::winuser::{PostThreadMessageA, WM_APP, WM_QUIT};

#[test]
fn windows_keyboard_listener_survives_posted_messages() {
    let (thread_tx, thread_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        // SAFETY: GetCurrentThreadId has no preconditions.
        thread_tx.send(unsafe { GetCurrentThreadId() }).unwrap();
        let result = rdev::listen(|_| {});
        done_tx.send(result).unwrap();
    });
    let thread_id = thread_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    // Installing the hooks and creating the message queue happens asynchronously.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // SAFETY: Posting a benign application message to the listener's known thread.
        if unsafe { PostThreadMessageA(thread_id, WM_APP, 0, 0) } != 0 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "listener message queue was not created"
        );
        assert!(
            matches!(done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
            "listener failed to start"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let early_result = done_rx.recv_timeout(Duration::from_millis(100));
    // SAFETY: Ask the listener thread to leave its message loop without disturbing the UI.
    assert_ne!(unsafe { PostThreadMessageA(thread_id, WM_QUIT, 0, 0) }, 0);
    assert!(
        matches!(early_result, Err(mpsc::RecvTimeoutError::Timeout)),
        "listener exited after an ordinary posted message: {early_result:?}"
    );
    done_rx
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    worker.join().unwrap();
}
