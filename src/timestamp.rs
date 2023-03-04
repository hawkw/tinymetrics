use core::fmt;
use portable_atomic::{AtomicU64, Ordering};

/// A Unix timestamp, represented in seconds since the Unix epoch.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct UnixTimestamp(u64);

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub(crate) struct TimestampCell {
    now: AtomicU64,
    #[cfg_attr(feature = "serde", serde(skip))]
    timestamp_fn: fn() -> UnixTimestamp,
}

// === impl UnixTimestamp ===

impl UnixTimestamp {
    pub fn from_secs(secs: u64) -> Self {
        Self(secs)
    }

    #[cfg(feature = "std")]
    pub fn from_std(time: std::time::SystemTime) -> Self {
        Self(
            time.duration_since(std::time::UNIX_EPOCH)
                .expect("system time is before the start of the Unix epoch!")
                .as_secs(),
        )
    }

    #[cfg(feature = "std")]
    pub fn now() -> Self {
        Self::from_std(std::time::SystemTime::now())
    }

    pub(crate) fn as_secs(self) -> u64 {
        self.0
    }
}

#[cfg(feature = "std")]
impl From<std::time::SystemTime> for UnixTimestamp {
    fn from(time: std::time::SystemTime) -> Self {
        Self::from_std(time)
    }
}

impl fmt::Display for UnixTimestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// === impl TimestampCell ===

impl TimestampCell {
    pub(crate) const fn new(timestamp_fn: fn() -> UnixTimestamp) -> Self {
        Self {
            now: AtomicU64::new(0),
            timestamp_fn,
        }
    }

    pub(crate) fn update_max(&self) {
        let now = (self.timestamp_fn)().as_secs();
        self.now.fetch_max(now, Ordering::AcqRel);
    }

    pub(crate) fn update_if_ahead(&self) -> bool {
        let now = (self.timestamp_fn)().as_secs();
        let mut curr = self.now.load(Ordering::Relaxed);
        loop {
            if now <= curr {
                return false;
            }

            match self
                .now
                .compare_exchange_weak(curr, now, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return true,
                Err(actual) => curr = actual,
            }
        }
    }

    pub(crate) fn timestamp(&self) -> UnixTimestamp {
        UnixTimestamp::from_secs(self.now.load(Ordering::Relaxed))
    }
}

impl fmt::Display for TimestampCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.timestamp())
    }
}

impl fmt::Debug for TimestampCell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.timestamp())
    }
}
