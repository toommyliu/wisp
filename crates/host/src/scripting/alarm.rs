use std::io;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

#[cfg(not(target_os = "macos"))]
pub(crate) struct Alarm(std::sync::Condvar);

#[cfg(not(target_os = "macos"))]
impl Alarm {
    pub(crate) fn new() -> io::Result<Self> {
        Ok(Self(std::sync::Condvar::new()))
    }

    pub(crate) fn ring(&self) {
        self.0.notify_one();
    }

    pub(crate) fn wait<'a, T>(
        &self,
        _lock: &'a Mutex<T>,
        guard: MutexGuard<'a, T>,
        until: Option<Instant>,
    ) -> MutexGuard<'a, T> {
        match until {
            Some(until) => {
                self.0
                    .wait_timeout(guard, until.saturating_duration_since(Instant::now()))
                    .unwrap()
                    .0
            }
            None => self.0.wait(guard).unwrap(),
        }
    }
}

// Windows runs a ready thread ahead of a running one only at a higher priority, so a watchdog at
// the script thread's priority waits behind the runaway it stops whenever every core is busy.
#[cfg(target_os = "windows")]
pub(crate) fn outrank_script_threads() {
    use std::os::windows::io::RawHandle;
    const THREAD_PRIORITY_HIGHEST: i32 = 2;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThread() -> RawHandle;
        fn SetThreadPriority(thread: RawHandle, priority: i32) -> i32;
    }
    if unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST) } == 0 {
        let error = io::Error::last_os_error();
        tracing::warn!(%error, "could not raise the watchdog's thread priority");
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn outrank_script_threads() {}

// A background or App Napped process gets its timers coalesced by up to about 100 ms, condvar
// timeouts included. A kqueue timer marked critical still fires on time.
#[cfg(target_os = "macos")]
pub(crate) struct Alarm(std::os::fd::OwnedFd);

#[cfg(target_os = "macos")]
impl Alarm {
    const RING: usize = 0;
    const TIMER: usize = 1;

    pub(crate) fn new() -> io::Result<Self> {
        use std::os::fd::FromRawFd;
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return Err(io::Error::last_os_error());
        }
        let alarm = Self(unsafe { std::os::fd::OwnedFd::from_raw_fd(queue) });
        alarm.kevent(
            &[event(
                Self::RING,
                libc::EVFILT_USER,
                libc::EV_ADD | libc::EV_CLEAR,
                0,
                0,
            )],
            false,
        )?;
        Ok(alarm)
    }

    pub(crate) fn ring(&self) {
        let _ = self.kevent(
            &[event(
                Self::RING,
                libc::EVFILT_USER,
                0,
                libc::NOTE_TRIGGER,
                0,
            )],
            false,
        );
    }

    pub(crate) fn wait<'a, T>(
        &self,
        lock: &'a Mutex<T>,
        guard: MutexGuard<'a, T>,
        until: Option<Instant>,
    ) -> MutexGuard<'a, T> {
        let timer = until.map(|until| {
            event(
                Self::TIMER,
                libc::EVFILT_TIMER,
                libc::EV_ADD | libc::EV_ONESHOT,
                libc::NOTE_NSECONDS | libc::NOTE_CRITICAL,
                until.saturating_duration_since(Instant::now()).as_nanos() as isize,
            )
        });
        drop(guard);
        let _ = self.kevent(timer.as_slice(), true);
        lock.lock().unwrap()
    }

    fn kevent(&self, changes: &[libc::kevent], wait: bool) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        let mut fired = [event(0, 0, 0, 0, 0)];
        let capacity = if wait { 1 } else { 0 };
        let result = unsafe {
            libc::kevent(
                self.0.as_raw_fd(),
                changes.as_ptr(),
                changes.len() as i32,
                fired.as_mut_ptr(),
                capacity,
                std::ptr::null(),
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn event(ident: usize, filter: i16, flags: u16, fflags: u32, data: isize) -> libc::kevent {
    libc::kevent {
        ident,
        filter,
        flags,
        fflags,
        data,
        udata: std::ptr::null_mut(),
    }
}
