//! Shared, thread-scoped archive limits, checked inside the pinned decoders.
//! Charges are cumulative (not estimates of process RSS). No password or path is stored.
use std::{
    cell::RefCell,
    io,
    marker::PhantomData,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
pub const MEMORY: usize = 512 * 1024 * 1024;
pub const HEADER: usize = 16 * 1024 * 1024;
pub const DICTIONARY: usize = 64 * 1024 * 1024;
pub const ENTRIES: usize = 100_000;
pub const WORK: u64 = 32_000_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    Limit,
    Cancelled,
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Limit => "archive decoder budget exceeded",
            Self::Cancelled => "archive decoding cancelled",
        })
    }
}
impl std::error::Error for Failure {}
struct State {
    entries: usize,
    bytes: usize,
    work: u64,
    end: Instant,
    cancel: Arc<AtomicBool>,
    failure: Option<Failure>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            entries: 0,
            bytes: 0,
            work: 0,
            end: Instant::now() + Duration::from_secs(1800),
            cancel: Arc::new(AtomicBool::new(false)),
            failure: None,
        }
    }
}
thread_local! { static STATE: RefCell<State> = RefCell::new(State::default()); }
/// Must stay on the decoder thread; drop restores the enclosing scope.
pub struct Scope {
    previous: Option<State>,
    _thread: PhantomData<Rc<()>>,
}
impl Scope {
    pub fn new(cancel: Arc<AtomicBool>, duration: Duration) -> Self {
        let state = State {
            cancel,
            end: Instant::now() + duration,
            ..State::default()
        };
        Self {
            previous: Some(STATE.with(|s| s.replace(state))),
            _thread: PhantomData,
        }
    }
}
impl Drop for Scope {
    fn drop(&mut self) {
        if let Some(old) = self.previous.take() {
            STATE.with(|s| s.replace(old));
        }
    }
}
pub fn fail() -> io::Error {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.failure.get_or_insert(Failure::Limit);
        io::Error::other(s.failure.unwrap())
    })
}
pub fn check() -> io::Result<()> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.work = s.work.saturating_add(1);
        if s.cancel.load(Ordering::Relaxed) {
            s.failure = Some(Failure::Cancelled);
        } else if s.work > WORK || Instant::now() >= s.end {
            s.failure.get_or_insert(Failure::Limit);
        }
        match s.failure {
            Some(error) => Err(io::Error::other(error)),
            None => Ok(()),
        }
    })
}
pub fn charge(count: usize, unit: usize) -> io::Result<()> {
    check()?;
    let bytes = count.checked_mul(unit).ok_or_else(fail)?;
    let allowed = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.bytes = s.bytes.saturating_add(bytes);
        s.bytes <= MEMORY
    });
    if allowed {
        Ok(())
    } else {
        Err(fail())
    }
}
pub fn count(count: usize) -> io::Result<()> {
    check()?;
    if count > ENTRIES {
        Err(fail())
    } else {
        Ok(())
    }
}
pub fn header(bytes: usize) -> io::Result<()> {
    if bytes > HEADER {
        return Err(fail());
    }
    charge(bytes, 1)
}
/// Cancellation/deadline checks also apply inside upstream decoder input loops.
pub struct Reader<R>(pub R);
impl<R: io::Read> io::Read for Reader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        check()?;
        let size = out.len().min(64 * 1024);
        self.0.read(&mut out[..size])
    }
}
impl<R: io::Seek> io::Seek for Reader<R> {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        check()?;
        self.0.seek(pos)
    }
}
/// A live decoder allocation; unlike metadata work it can be released between entries.
pub struct Reservation(usize);
pub fn reserve(bytes: usize) -> io::Result<Reservation> {
    charge(bytes, 1)?;
    Ok(Reservation(bytes))
}
impl Drop for Reservation {
    fn drop(&mut self) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.bytes = s.bytes.saturating_sub(self.0);
        });
    }
}
pub struct ReservedReader<R> {
    pub inner: R,
    pub reservation: Reservation,
}
impl<R: io::Read> io::Read for ReservedReader<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        check()?;
        let size = out.len().min(64 * 1024);
        self.inner.read(&mut out[..size])
    }
}

pub fn entry() -> io::Result<()> {
    let number = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.entries = s.entries.saturating_add(1);
        s.entries
    });
    count(number)
}

pub fn work(units: u64) -> io::Result<()> {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.work = s.work.saturating_add(units);
    });
    check()
}
