// SPDX-License-Identifier: GPL-3.0-or-later
//! Failed sign-ins, counted per peer address, for every server Deckle runs (the web UI, the
//! reader server, calibre's wireless connection). A peer may get its password wrong a few times
//! for free; after that each failure doubles its wait. One peer guessing never keeps another
//! out, so the owner can still sign in while someone else is guessing.

use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv6Addr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Failures a peer may have before it has to wait.
pub const FREE_FAILURES: u32 = 5;
/// The first wait, doubled by each failure after that.
pub const FIRST_WAIT: Duration = Duration::from_secs(1);
/// The longest wait.
pub const MAX_WAIT: Duration = Duration::from_secs(5 * 60);
/// Sign-ins a peer may have being checked at once.
pub const IN_FLIGHT: u8 = 2;
/// Failures a minute, from everyone, past which each further failure is answered a second late.
pub const BUSY_FAILURES: usize = 100;
/// Peers kept; the longest idle are dropped past this.
pub const MAX_PEERS: usize = 10_000;

/// What the limiter reads the time from: the clock, or a test's.
pub type Clock = Arc<dyn Fn() -> Instant + Send + Sync>;

/// Why a peer may not try now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// It is waiting out its failures, for this much longer.
    Wait(Duration),
    /// It already has [`IN_FLIGHT`] sign-ins being checked.
    Busy,
}

impl Refused {
    /// Seconds for `Retry-After`, at least 1.
    pub fn retry_after_secs(self) -> u64 {
        match self {
            Refused::Wait(d) => d.as_secs_f64().ceil().max(1.0) as u64,
            Refused::Busy => 1,
        }
    }
}

#[derive(Default)]
struct Peer {
    failures: u32,
    blocked_until: Option<Instant>,
    in_flight: u8,
    last_seen: Option<Instant>,
    last_logged: Option<Instant>,
    unlogged: u32,
}

struct State {
    peers: HashMap<IpAddr, Peer>,
    recent: VecDeque<Instant>,
}

pub struct FailureLimiter {
    /// Named in the log: "web UI", "reader server", ….
    server: &'static str,
    clock: Clock,
    state: Mutex<State>,
}

/// The key a peer is counted under: an IPv4 address as itself even when it arrives as
/// `::ffff:a.b.c.d`, and an IPv6 address by its /64, which one household or host usually holds.
pub fn peer_key(ip: IpAddr) -> IpAddr {
    match ip.to_canonical() {
        IpAddr::V6(v6) => {
            let s = v6.segments();
            IpAddr::V6(Ipv6Addr::new(s[0], s[1], s[2], s[3], 0, 0, 0, 0))
        }
        v4 => v4,
    }
}

/// How long a peer waits after `failures` failures.
fn wait_after(failures: u32) -> Option<Duration> {
    let over = failures.checked_sub(FREE_FAILURES)?.checked_sub(1)?;
    let wait = FIRST_WAIT.saturating_mul(1u32.checked_shl(over).unwrap_or(u32::MAX));
    Some(wait.min(MAX_WAIT))
}

impl FailureLimiter {
    pub fn new(server: &'static str) -> FailureLimiter {
        Self::with_clock(server, Arc::new(Instant::now))
    }

    pub fn with_clock(server: &'static str, clock: Clock) -> FailureLimiter {
        FailureLimiter {
            server,
            clock,
            state: Mutex::new(State {
                peers: HashMap::new(),
                recent: VecDeque::new(),
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Reserves a sign-in for `peer`, before its password is checked. The [`Attempt`] counts as
    /// a failure when dropped, unless [`Attempt::succeeded`] was called, so a client that hangs
    /// up mid-check still pays for its guess.
    pub fn begin(&self, peer: IpAddr) -> Result<Attempt<'_>, Refused> {
        let key = peer_key(peer);
        let now = (self.clock)();
        let mut st = self.lock();
        if !st.peers.contains_key(&key) && st.peers.len() >= MAX_PEERS {
            evict(&mut st.peers, now);
        }
        let p = st.peers.entry(key).or_default();
        p.last_seen = Some(now);
        let refused = match p.blocked_until {
            Some(until) if until > now => Some(Refused::Wait(until - now)),
            _ if p.in_flight >= IN_FLIGHT => Some(Refused::Busy),
            _ => None,
        };
        if let Some(r) = refused {
            self.log(p, key, now, "refused");
            return Err(r);
        }
        p.in_flight += 1;
        Ok(Attempt {
            limiter: self,
            key,
            done: false,
        })
    }

    /// Peers being tracked.
    pub fn peers(&self) -> usize {
        self.lock().peers.len()
    }

    /// One log line per peer a minute, with how many it has had since the last.
    fn log(&self, p: &mut Peer, key: IpAddr, now: Instant, what: &str) {
        p.unlogged += 1;
        let due = p
            .last_logged
            .is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_secs(60));
        if due {
            eprintln!(
                "{}: {what} sign-in from {key} ({} since the last note, {} failures in a row)",
                self.server, p.unlogged, p.failures
            );
            p.last_logged = Some(now);
            p.unlogged = 0;
        }
    }

    fn finish(&self, key: IpAddr, ok: bool) -> Duration {
        let now = (self.clock)();
        let mut st = self.lock();
        let mut extra = Duration::ZERO;
        if !ok {
            let minute_ago = now.checked_sub(Duration::from_secs(60));
            while st
                .recent
                .front()
                .is_some_and(|t| minute_ago.is_some_and(|m| *t <= m))
            {
                st.recent.pop_front();
            }
            st.recent.push_back(now);
            if st.recent.len() > BUSY_FAILURES {
                extra = FIRST_WAIT;
            }
        }
        let Some(p) = st.peers.get_mut(&key) else {
            return extra;
        };
        p.in_flight = p.in_flight.saturating_sub(1);
        p.last_seen = Some(now);
        if ok {
            p.failures = 0;
            p.blocked_until = None;
        } else {
            p.failures = p.failures.saturating_add(1);
            p.blocked_until = wait_after(p.failures).map(|w| now + w);
            self.log(p, key, now, "failed");
        }
        extra
    }
}

/// Drops the longest-idle peers that have nothing in flight, to make room for one more.
fn evict(peers: &mut HashMap<IpAddr, Peer>, now: Instant) {
    // Peers whose wait is over and who were last seen ten minutes ago or more go first.
    peers.retain(|_, p| {
        p.in_flight > 0
            || p.blocked_until.is_some_and(|u| u > now)
            || p.last_seen
                .is_some_and(|t| now.saturating_duration_since(t) < Duration::from_secs(600))
    });
    // Then the longest idle, down to nine tenths full, so this runs once per thousand new
    // peers rather than for each.
    let target = MAX_PEERS * 9 / 10;
    if peers.len() >= MAX_PEERS {
        let mut idle: Vec<(Option<Instant>, IpAddr)> = peers
            .iter()
            .filter(|(_, p)| p.in_flight == 0)
            .map(|(k, p)| (p.last_seen, *k))
            .collect();
        idle.sort_unstable();
        let excess = peers.len() - target;
        for (_, k) in idle.into_iter().take(excess) {
            peers.remove(&k);
        }
    }
}

/// One sign-in being checked. Dropped without [`succeeded`](Self::succeeded), it is a failure.
pub struct Attempt<'a> {
    limiter: &'a FailureLimiter,
    key: IpAddr,
    done: bool,
}

impl Attempt<'_> {
    /// The password was right: the peer's failures are forgotten.
    pub fn succeeded(mut self) {
        self.done = true;
        self.limiter.finish(self.key, true);
    }

    /// The password was wrong. Returns how long to hold the answer back: nothing, unless
    /// failures from everyone are past [`BUSY_FAILURES`] a minute.
    pub fn failed(mut self) -> Duration {
        self.done = true;
        self.limiter.finish(self.key, false)
    }
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if !self.done {
            self.limiter.finish(self.key, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestClock(Arc<Mutex<Instant>>);

    impl TestClock {
        fn new() -> (TestClock, Clock) {
            let t = Arc::new(Mutex::new(Instant::now()));
            let read = t.clone();
            (TestClock(t), Arc::new(move || *read.lock().unwrap()))
        }
        fn advance(&self, d: Duration) {
            *self.0.lock().unwrap() += d;
        }
    }

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn five_failures_are_free_then_the_wait_doubles_to_five_minutes() {
        let (clock, read) = TestClock::new();
        let l = FailureLimiter::with_clock("test", read);
        let a = ip("192.168.1.9");
        for _ in 0..FREE_FAILURES {
            l.begin(a).unwrap().failed();
        }
        // Still free to try: the 6th failure is what starts the wait.
        let mut expected = Duration::from_secs(1);
        for _ in 0..12 {
            l.begin(a).unwrap().failed();
            assert_eq!(l.begin(a).err(), Some(Refused::Wait(expected)));
            clock.advance(expected);
            expected = (expected * 2).min(MAX_WAIT);
        }
        assert_eq!(expected, MAX_WAIT);
    }

    #[test]
    fn a_success_clears_the_peer() {
        let (clock, read) = TestClock::new();
        let l = FailureLimiter::with_clock("test", read);
        let a = ip("10.0.0.2");
        for _ in 0..6 {
            l.begin(a).unwrap().failed();
        }
        assert!(l.begin(a).is_err());
        clock.advance(Duration::from_secs(1));
        l.begin(a).unwrap().succeeded();
        for _ in 0..FREE_FAILURES {
            l.begin(a).unwrap().failed();
        }
        assert!(l.begin(a).is_ok());
    }

    #[test]
    fn one_peer_guessing_never_holds_up_another() {
        let l = FailureLimiter::new("test");
        let guesser = ip("203.0.113.7");
        for _ in 0..20 {
            if let Ok(a) = l.begin(guesser) {
                a.failed();
            }
        }
        assert!(l.begin(guesser).is_err());
        l.begin(ip("192.168.1.2")).unwrap().succeeded();
    }

    #[test]
    fn at_most_two_checks_in_flight_per_peer() {
        let l = FailureLimiter::new("test");
        let a = ip("192.168.1.9");
        let one = l.begin(a).unwrap();
        let two = l.begin(a).unwrap();
        assert_eq!(l.begin(a).err(), Some(Refused::Busy));
        assert!(l.begin(ip("192.168.1.10")).is_ok());
        drop(one); // a hang-up counts as a failure, and frees the slot
        let three = l.begin(a).unwrap();
        two.succeeded();
        three.succeeded();
    }

    #[test]
    fn a_dropped_attempt_is_a_failure() {
        let l = FailureLimiter::new("test");
        let a = ip("192.168.1.9");
        for _ in 0..6 {
            drop(l.begin(a).unwrap());
        }
        assert!(matches!(l.begin(a), Err(Refused::Wait(_))));
    }

    #[test]
    fn mapped_v4_is_its_own_v4_and_v6_counts_by_its_64() {
        assert_eq!(peer_key(ip("::ffff:192.168.1.9")), ip("192.168.1.9"));
        assert_ne!(
            peer_key(ip("::ffff:192.168.1.9")),
            peer_key(ip("::ffff:192.168.1.10"))
        );
        assert_eq!(
            peer_key(ip("2001:db8:1:2:aaaa::1")),
            peer_key(ip("2001:db8:1:2:bbbb::9"))
        );
        assert_ne!(
            peer_key(ip("2001:db8:1:2::1")),
            peer_key(ip("2001:db8:1:3::1"))
        );
        let l = FailureLimiter::new("test");
        for _ in 0..6 {
            l.begin(ip("2001:db8:1:2::1")).unwrap().failed();
        }
        assert!(l.begin(ip("2001:db8:1:2::77")).is_err());
        assert!(l.begin(ip("::ffff:10.0.0.1")).is_ok());
    }

    #[test]
    fn many_failures_from_everyone_slow_failures_but_not_successes() {
        let l = FailureLimiter::new("test");
        for i in 0..BUSY_FAILURES {
            let peer = IpAddr::from([10, 1, (i / 250) as u8, (i % 250) as u8]);
            assert_eq!(l.begin(peer).unwrap().failed(), Duration::ZERO);
        }
        assert_eq!(l.begin(ip("10.9.9.9")).unwrap().failed(), FIRST_WAIT);
        // A right password is never held back, and isn't counted.
        l.begin(ip("10.9.9.8")).unwrap().succeeded();
    }

    #[test]
    fn the_table_holds_at_most_ten_thousand_peers() {
        let l = FailureLimiter::new("test");
        for i in 0..20_000u32 {
            let peer = IpAddr::from(std::net::Ipv4Addr::from(0x0a00_0000u32 + i));
            l.begin(peer).unwrap().failed();
        }
        assert!(l.peers() <= MAX_PEERS, "{}", l.peers());
    }
}
