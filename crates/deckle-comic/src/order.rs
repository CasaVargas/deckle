// SPDX-License-Identifier: GPL-3.0-or-later
//! Natural page order: folder by folder, case-insensitive, digit runs compared as numbers, so
//! `page2` comes before `page10` and `Chapter 9/` before `Chapter 10/`.

use std::cmp::Ordering;

/// Sorts `/`-separated paths in natural order.
pub fn natural_sort(names: &mut [String]) {
    names.sort_by(|a, b| compare_paths(a, b));
}

fn compare_paths(a: &str, b: &str) -> Ordering {
    let mut sa = a.split('/');
    let mut sb = b.split('/');
    loop {
        match (sa.next(), sb.next()) {
            (Some(x), Some(y)) => match compare(x, y) {
                Ordering::Equal => continue,
                o => return o,
            },
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
}

/// One path segment against another; ties broken by the raw text so the order is total.
fn compare(a: &str, b: &str) -> Ordering {
    let (mut ca, mut cb) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ca.peek().copied(), cb.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na = digits(&mut ca);
                let nb = digits(&mut cb);
                // Without leading zeros, a longer run is a bigger number.
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                match ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb)) {
                    Ordering::Equal => continue,
                    o => return o,
                }
            }
            (Some(x), Some(y)) => {
                let (lx, ly) = (x.to_lowercase(), y.to_lowercase());
                match lx.cmp(ly) {
                    Ordering::Equal => {
                        ca.next();
                        cb.next();
                    }
                    o => return o,
                }
            }
        }
    }
}

fn digits(it: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        s.push(c);
        it.next();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::natural_sort;

    fn sorted(names: &[&str]) -> Vec<String> {
        let mut v: Vec<String> = names.iter().map(|s| s.to_string()).collect();
        natural_sort(&mut v);
        v
    }

    #[test]
    fn numbers_compare_as_numbers() {
        assert_eq!(
            sorted(&["page10.jpg", "page2.jpg", "page1.jpg", "Page3.jpg"]),
            ["page1.jpg", "page2.jpg", "Page3.jpg", "page10.jpg"]
        );
    }

    #[test]
    fn leading_zeros_and_long_runs() {
        assert_eq!(
            sorted(&["010.png", "9.png", "0001.png", "12345678901234567890.png"]),
            ["0001.png", "9.png", "010.png", "12345678901234567890.png"]
        );
    }

    #[test]
    fn folders_sort_first_by_folder() {
        assert_eq!(
            sorted(&["Ch 10/01.jpg", "Ch 9/02.jpg", "Ch 9/01.jpg", "cover.jpg"]),
            ["Ch 9/01.jpg", "Ch 9/02.jpg", "Ch 10/01.jpg", "cover.jpg"]
        );
    }
}
