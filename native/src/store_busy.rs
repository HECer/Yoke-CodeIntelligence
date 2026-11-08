//! Per-connection SQLite lock waits use actual time and the host cancellation
//! flags. SQLite's built-in timeout counts requested sleeps, not elapsed time.
//! The locked bundled SQLite provides connection-owned client data (3.44+).
use crate::capture::Deadline;
use anyhow::{Result, anyhow};
use rusqlite::{Connection, ffi};
use std::{
    ffi::{c_int, c_void},
    sync::Mutex,
    time::{Duration, Instant},
};

const KEY: &std::ffi::CStr = c"yoke.store.busy_deadline.v1";
const MAX_WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(2);

struct Policy {
    deadline: Option<Deadline>,
    event_end: Option<Instant>,
}
struct State {
    policy: Mutex<Policy>,
}

unsafe extern "C" fn destroy(data: *mut c_void) {
    // sqlite3_set_clientdata owns this allocation, including on registration
    // failure, and invokes the destructor exactly once. No Store borrow escapes.
    unsafe { drop(Box::from_raw(data.cast::<State>())) };
}

unsafe extern "C" fn wait(data: *mut c_void, count: c_int) -> c_int {
    std::panic::catch_unwind(|| {
        if data.is_null() || count < 0 {
            return 0;
        }
        // The callback argument is SQLite's own connection, not a Store or State
        // pointer. Read-only client-data lookup also returns null once closing
        // removes the registration, so callback state cannot outlive its owner.
        // This performs no SQL and never changes the invoking connection.
        let raw = unsafe { ffi::sqlite3_get_clientdata(data.cast(), KEY.as_ptr()) };
        let Some(state) = (unsafe { raw.cast::<State>().as_ref() }) else {
            return 0;
        };
        let (end, deadline) = {
            let Ok(mut policy) = state.policy.lock() else {
                return 0;
            };
            if count == 0 || policy.event_end.is_none() {
                policy.event_end = Some(Instant::now() + MAX_WAIT);
            }
            (policy.event_end.unwrap(), policy.deadline.clone())
        };
        let remaining = || {
            let elapsed_bound = end.saturating_duration_since(Instant::now());
            deadline
                .as_ref()
                .map_or(elapsed_bound, |d| elapsed_bound.min(d.remaining()))
        };
        let duration = remaining();
        if duration.is_zero() {
            return 0;
        }
        std::thread::sleep(duration.min(POLL));
        c_int::from(!remaining().is_zero())
    })
    .unwrap_or(0)
}

fn result(code: c_int) -> Result<()> {
    if code != ffi::SQLITE_OK {
        return Err(rusqlite::Error::SqliteFailure(ffi::Error::new(code), None).into());
    }
    Ok(())
}

pub(super) fn install(db: &Connection, deadline: Option<&Deadline>) -> Result<()> {
    // Connection is not Sync. These calls run with its live owner on the same
    // thread, outside a SQLite callback. State belongs to SQLite, so moving a
    // public Store::db out of Store cannot leave a dangling callback argument.
    let handle = unsafe { db.handle() };
    let mut data = unsafe { ffi::sqlite3_get_clientdata(handle, KEY.as_ptr()) };
    if data.is_null() {
        data = Box::into_raw(Box::new(State {
            policy: Mutex::new(Policy {
                deadline: None,
                event_end: None,
            }),
        }))
        .cast();
        result(unsafe { ffi::sqlite3_set_clientdata(handle, KEY.as_ptr(), data, Some(destroy)) })?;
    }
    let state = unsafe { &*data.cast::<State>() };
    *state
        .policy
        .lock()
        .map_err(|_| anyhow!("SQLite lock-wait policy unavailable"))? = Policy {
        deadline: deadline.cloned(),
        event_end: None,
    };
    result(unsafe { ffi::sqlite3_busy_handler(handle, Some(wait), handle.cast()) })
}

pub(super) fn replace_if_present(db: &Connection, deadline: Option<&Deadline>) {
    let data = unsafe { ffi::sqlite3_get_clientdata(db.handle(), KEY.as_ptr()) };
    if let Some(state) = unsafe { data.cast::<State>().as_ref() }
        && let Ok(mut policy) = state.policy.lock()
    {
        *policy = Policy {
            deadline: deadline.cloned(),
            event_end: None,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removed_client_data_leaves_a_fail_closed_callback_and_can_be_reinstalled() {
        let db = Connection::open_in_memory().unwrap();
        install(&db, None).unwrap();
        let handle = unsafe { db.handle() };
        assert!(!unsafe { ffi::sqlite3_get_clientdata(handle, KEY.as_ptr()) }.is_null());
        // SQLite invokes the destructor here. A later callback must look up the
        // connection again, not dereference the freed registration allocation.
        result(unsafe {
            ffi::sqlite3_set_clientdata(handle, KEY.as_ptr(), std::ptr::null_mut(), None)
        })
        .unwrap();
        assert_eq!(unsafe { wait(handle.cast(), 0) }, 0);
        install(&db, Some(&Deadline::new(0))).unwrap();
        assert_eq!(unsafe { wait(handle.cast(), 0) }, 0);
        db.close().unwrap();
    }
}
