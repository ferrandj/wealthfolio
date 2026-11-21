//! Holding the processing of domain events while a long run of changes goes on (many documents read again): the
//! events are still collected, and handled once the run is over, instead of the portfolio being valued again after
//! each document.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static HOLDS: AtomicUsize = AtomicUsize::new(0);
/// When the last hold ended (milliseconds since the epoch).
static RELEASED_AT: AtomicU64 = AtomicU64::new(0);

/// How long the events still wait after a hold ends: an update done in parts starts its next part within that time,
/// so the portfolio is not valued again between two parts.
pub const HOLD_GRACE_MS: u64 = 3_000;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// While one is alive (and for `HOLD_GRACE_MS` after the last one ends), the event workers keep collecting events
/// without handling them.
pub struct EventHold(());

impl EventHold {
    pub fn new() -> Self {
        HOLDS.fetch_add(1, Ordering::SeqCst);
        Self(())
    }
}

impl Default for EventHold {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for EventHold {
    fn drop(&mut self) {
        RELEASED_AT.store(now_ms(), Ordering::SeqCst);
        HOLDS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Are the events to wait?
pub fn events_held() -> bool {
    HOLDS.load(Ordering::SeqCst) > 0
        || now_ms().saturating_sub(RELEASED_AT.load(Ordering::SeqCst)) < HOLD_GRACE_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_wait_while_held_and_a_little_after() {
        {
            let _hold = EventHold::new();
            assert!(events_held());
        }
        // just released: still waiting for a next part
        assert!(events_held());
    }
}
