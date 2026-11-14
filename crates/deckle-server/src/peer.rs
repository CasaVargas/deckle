// SPDX-License-Identifier: GPL-3.0-or-later
//! Whose sign-in this is: the socket's address, or, when that is a proxy the owner listed in
//! `DECKLE_WEB_TRUSTED_PROXIES`, the address the proxy says it is passing on. Without that
//! setting `X-Forwarded-For` is ignored, since any client can write it.

use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::http::HeaderMap;

/// An address, or a range of them: `10.0.0.5`, `172.18.0.0/16`, `fd00::/8`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Range {
    pub addr: IpAddr,
    pub prefix: u8,
}

impl Range {
    pub fn parse(s: &str) -> Result<Range, String> {
        let s = s.trim();
        let (addr, prefix) = match s.split_once('/') {
            Some((a, p)) => (a, Some(p)),
            None => (s, None),
        };
        let addr: IpAddr = addr
            .parse()
            .map_err(|_| format!("not an address or range: {s}"))?;
        let addr = addr.to_canonical();
        let max = if addr.is_ipv4() { 32 } else { 128 };
        let prefix = match prefix {
            Some(p) => p
                .parse::<u8>()
                .ok()
                .filter(|p| *p <= max)
                .ok_or_else(|| format!("not an address or range: {s}"))?,
            None => max,
        };
        Ok(Range { addr, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
        match (self.addr, ip.to_canonical()) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

/// One entry of `X-Forwarded-For`, without a port or brackets.
fn forwarded_addr(entry: &str) -> Option<IpAddr> {
    let e = entry.trim();
    if let Ok(ip) = e.parse::<IpAddr>() {
        return Some(ip);
    }
    if let Some(rest) = e.strip_prefix('[') {
        return rest.split_once(']')?.0.parse().ok();
    }
    let (host, port) = e.rsplit_once(':')?;
    port.parse::<u16>().ok()?;
    host.parse::<std::net::Ipv4Addr>().ok().map(IpAddr::V4)
}

static WARNED: AtomicBool = AtomicBool::new(false);

/// The address a sign-in is counted against.
pub fn client(socket: IpAddr, headers: &HeaderMap, trusted: &[Range]) -> IpAddr {
    let socket = socket.to_canonical();
    let has_header = headers.contains_key("x-forwarded-for");
    if trusted.is_empty() {
        if has_header
            && (socket.is_loopback() || is_private(socket))
            && !WARNED.swap(true, Ordering::Relaxed)
        {
            eprintln!(
                "web UI: a proxy seems to be in front (X-Forwarded-For from {socket}): set \
                 DECKLE_WEB_TRUSTED_PROXIES to its address, or every sign-in counts as the \
                 proxy's and one wrong guess slows everyone's"
            );
        }
        return socket;
    }
    if !trusted.iter().any(|r| r.contains(socket)) {
        return socket;
    }
    // A proxy may add a line rather than append to one: all lines, in order, right to left.
    let entries: Vec<&str> = headers
        .get_all("x-forwarded-for")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(','))
        .collect();
    for entry in entries.iter().rev() {
        let Some(ip) = forwarded_addr(entry) else {
            // Something we can't read: trust nothing to its left.
            return socket;
        };
        let ip = ip.to_canonical();
        if !trusted.iter().any(|r| r.contains(ip)) {
            return ip;
        }
    }
    socket
}

fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => (v6.segments()[0] & 0xfe00) == 0xfc00,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    fn headers(lines: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for l in lines {
            h.append("x-forwarded-for", l.parse().unwrap());
        }
        h
    }

    #[test]
    fn ranges() {
        let r = Range::parse("172.18.0.0/16").unwrap();
        assert!(r.contains(ip("172.18.4.5")));
        assert!(r.contains(ip("::ffff:172.18.4.5")));
        assert!(!r.contains(ip("172.19.0.1")));
        assert!(Range::parse("10.0.0.5").unwrap().contains(ip("10.0.0.5")));
        assert!(!Range::parse("10.0.0.5").unwrap().contains(ip("10.0.0.6")));
        assert!(Range::parse("fd00::/8").unwrap().contains(ip("fd12::1")));
        assert!(Range::parse("0.0.0.0/0").unwrap().contains(ip("8.8.8.8")));
        for bad in ["nope", "10.0.0.0/33", "fd00::/129", "10.0.0.0/x"] {
            assert!(Range::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_header_is_ignored_unless_the_socket_is_a_listed_proxy() {
        let h = headers(&["203.0.113.9"]);
        assert_eq!(client(ip("192.168.1.2"), &h, &[]), ip("192.168.1.2"));
        let proxy = [Range::parse("10.0.0.5").unwrap()];
        assert_eq!(client(ip("192.168.1.2"), &h, &proxy), ip("192.168.1.2"));
        assert_eq!(client(ip("10.0.0.5"), &h, &proxy), ip("203.0.113.9"));
    }

    #[test]
    fn the_right_most_untrusted_entry_counts() {
        let proxies = [
            Range::parse("10.0.0.5").unwrap(),
            Range::parse("10.0.0.6").unwrap(),
        ];
        // The client wrote the left-most entry itself.
        let h = headers(&["1.1.1.1, 203.0.113.9, 10.0.0.6"]);
        assert_eq!(client(ip("10.0.0.5"), &h, &proxies), ip("203.0.113.9"));
        // Two header lines: the last line's right-most entry.
        let h = headers(&["1.1.1.1", "203.0.113.9:4711"]);
        assert_eq!(client(ip("10.0.0.5"), &h, &proxies), ip("203.0.113.9"));
        let h = headers(&["[2001:db8::7]:443"]);
        assert_eq!(client(ip("10.0.0.5"), &h, &proxies), ip("2001:db8::7"));
        // Unreadable right-most: the socket, never something further left.
        let h = headers(&["203.0.113.9, unknown"]);
        assert_eq!(client(ip("10.0.0.5"), &h, &proxies), ip("10.0.0.5"));
        // Only proxies: the socket.
        let h = headers(&["10.0.0.6"]);
        assert_eq!(client(ip("10.0.0.5"), &h, &proxies), ip("10.0.0.5"));
    }
}
