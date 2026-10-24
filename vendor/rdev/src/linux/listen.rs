extern crate libc;
extern crate x11;
use crate::linux::common::{convert, FALSE};
use crate::linux::keyboard::Keyboard;
use crate::rdev::{Event, ListenError};
use std::cell::RefCell;
use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uchar, c_uint, c_ulong};
use std::ptr::{null, NonNull};
use x11::xlib;
use x11::xrecord;

struct ListenerState {
    keyboard: RefCell<Option<Keyboard>>,
    callback: RefCell<Box<dyn FnMut(Event)>>,
}

impl ListenerState {
    fn emit(&self, event: Event) {
        // A callback must not alias its own FnMut state if it reenters XRecord.
        if let Ok(mut callback) = self.callback.try_borrow_mut() {
            callback(event);
        }
    }
}

struct RecordedData(NonNull<xrecord::XRecordInterceptData>);

impl Drop for RecordedData {
    fn drop(&mut self) {
        // XRecord owns this allocation and requires XRecordFreeData, including
        // notifications that do not contain an input event.
        unsafe { xrecord::XRecordFreeData(self.0.as_ptr()) };
    }
}

pub fn listen<T>(callback: T) -> Result<(), ListenError>
where
    T: FnMut(Event) + 'static,
{
    let keyboard = Keyboard::new().ok_or(ListenError::KeyboardError)?;
    let state = ListenerState {
        keyboard: RefCell::new(Some(keyboard)),
        callback: RefCell::new(Box::new(callback)),
    };

    unsafe {
        // Open displays
        let dpy_control = xlib::XOpenDisplay(null());
        if dpy_control.is_null() {
            return Err(ListenError::MissingDisplayError);
        }
        let extension_name = CStr::from_bytes_with_nul(b"RECORD\0")
            .map_err(|_| ListenError::XRecordExtensionError)?;
        let extension = xlib::XInitExtension(dpy_control, extension_name.as_ptr());
        if extension.is_null() {
            xlib::XCloseDisplay(dpy_control);
            return Err(ListenError::XRecordExtensionError);
        }

        // Prepare record range
        // XRecordCreateContext copies the range; no Xlib allocation is needed.
        let mut record_range: xrecord::XRecordRange = std::mem::zeroed();
        record_range.device_events.first = xlib::KeyPress as c_uchar;
        record_range.device_events.last = xlib::MotionNotify as c_uchar;

        // Create context
        let mut clients: c_ulong = xrecord::XRecordAllClients;
        let mut range_pointer = &mut record_range as *mut xrecord::XRecordRange;
        let context =
            xrecord::XRecordCreateContext(dpy_control, 0, &mut clients, 1, &mut range_pointer, 1);

        if context == 0 {
            xlib::XCloseDisplay(dpy_control);
            return Err(ListenError::RecordContextError);
        }

        xlib::XSync(dpy_control, FALSE);
        // Run
        // The synchronous API invokes callbacks on this thread and does not
        // return until recording stops. The local state outlives all callbacks.
        let result = xrecord::XRecordEnableContext(
            dpy_control,
            context,
            Some(record_callback),
            (&state as *const ListenerState).cast_mut().cast::<c_char>(),
        );
        xrecord::XRecordFreeContext(dpy_control, context);
        xlib::XCloseDisplay(dpy_control);
        if result == 0 {
            return Err(ListenError::RecordContextEnablingError);
        }
    }
    Ok(())
}

// Core input events use the 32-byte X11 wire format, not Xlib's native XEvent.
// https://www.x.org/releases/X11R7.7/doc/libXtst/recordlib.html#Datum_Flags
// https://docs.rs/xproto/1.1.5/xproto/struct._xEvent__bindgen_ty_1.html
#[repr(C)]
struct XRecordDatum {
    type_: u8,
    code: u8,
    _header: [u8; 18],
    root_x: i16,
    root_y: i16,
    _tail: [u8; 8],
}

unsafe fn decode_record_data(data: &xrecord::XRecordInterceptData) -> Option<XRecordDatum> {
    if data.category != xrecord::XRecordFromServer
        || data.data.is_null()
        || data.data_len < (std::mem::size_of::<XRecordDatum>() / 4) as c_ulong
    {
        return None;
    }
    // The payload is a byte buffer and need not have native struct alignment.
    // Device events use the recording client's byte order, even when a recorded
    // client's protocol byte order differs (client_swapped).
    Some(std::ptr::read_unaligned(data.data.cast::<XRecordDatum>()))
}

unsafe extern "C" fn record_callback(
    user_data: *mut c_char,
    raw_data: *mut xrecord::XRecordInterceptData,
) {
    let Some(raw_data) = NonNull::new(raw_data) else {
        return;
    };
    let _guard = RecordedData(raw_data);
    let Some(xdatum) = decode_record_data(raw_data.as_ref()) else {
        return;
    };
    let Some(state) = (user_data as *const ListenerState).as_ref() else {
        return;
    };

    let code: c_uint = xdatum.code.into();
    let type_: c_int = xdatum.type_.into();

    let x = xdatum.root_x as f64;
    let y = xdatum.root_y as f64;

    let event = {
        let Ok(mut keyboard) = state.keyboard.try_borrow_mut() else {
            return;
        };
        convert(&mut keyboard, code, type_, x, y)
    };
    if let Some(event) = event {
        state.emit(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rdev::EventType;
    use std::cell::Cell;
    use std::rc::{Rc, Weak};
    use std::time::SystemTime;

    fn mouse_event() -> Event {
        Event {
            event_type: EventType::MouseMove { x: 12.0, y: -5.0 },
            time: SystemTime::now(),
            name: None,
        }
    }

    #[test]
    fn callbacks_keep_independent_state_across_repeated_events() {
        let first_count = Rc::new(Cell::new(0));
        let second_count = Rc::new(Cell::new(0));
        let listener = |count: Rc<Cell<usize>>| ListenerState {
            keyboard: RefCell::new(None),
            callback: RefCell::new(Box::new(move |_| count.set(count.get() + 1))),
        };
        let first = listener(first_count.clone());
        let second = listener(second_count.clone());
        first.emit(mouse_event());
        second.emit(mouse_event());
        first.emit(mouse_event());
        assert_eq!(first_count.get(), 2);
        assert_eq!(second_count.get(), 1);
    }

    #[test]
    fn reentrant_dispatch_does_not_alias_the_mutable_callback() {
        let calls = Rc::new(Cell::new(0));
        let state = Rc::new_cyclic(|weak: &Weak<ListenerState>| {
            let weak = weak.clone();
            let calls = calls.clone();
            ListenerState {
                keyboard: RefCell::new(None),
                callback: RefCell::new(Box::new(move |event| {
                    calls.set(calls.get() + 1);
                    weak.upgrade().unwrap().emit(event);
                })),
            }
        });
        state.emit(mouse_event());
        state.emit(mouse_event());
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn device_events_use_wire_offsets_and_notifications_have_no_payload() {
        assert_eq!(std::mem::size_of::<XRecordDatum>(), 32);
        let mut bytes = [0_u8; 33];
        bytes[1] = xlib::MotionNotify as u8;
        bytes[21..23].copy_from_slice(&123_i16.to_ne_bytes());
        bytes[23..25].copy_from_slice(&(-45_i16).to_ne_bytes());
        let mut data = xrecord::XRecordInterceptData {
            id_base: 0,
            server_time: 0,
            client_seq: 0,
            category: xrecord::XRecordFromServer,
            client_swapped: 1,
            data: unsafe { bytes.as_mut_ptr().add(1) },
            data_len: 8,
        };
        // The valid payload intentionally begins at an unaligned byte offset.
        let datum = unsafe { decode_record_data(&data) }.unwrap();
        assert_eq!(datum.type_, xlib::MotionNotify as u8);
        assert_eq!(datum.root_x, 123);
        assert_eq!(datum.root_y, -45);
        data.data_len = 7;
        assert!(unsafe { decode_record_data(&data) }.is_none());
        data.category = xrecord::XRecordStartOfData;
        data.data = std::ptr::null_mut();
        data.data_len = 0;
        assert!(unsafe { decode_record_data(&data) }.is_none());
    }
}
