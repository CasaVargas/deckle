// SPDX-License-Identifier: GPL-3.0-or-later
//! RTF bytes to a tree of groups and tokens (RTF spec §2, Input).

/// One token, or a `{…}` group of them.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Group(Vec<Node>),
    /// `\name` with its optional numeric parameter.
    Word(String, Option<i32>),
    /// `\` followed by one non-letter, e.g. `~`, `*`, `'`-less symbols, or a line break.
    Symbol(char),
    /// `\'hh`
    Hex(u8),
    /// Plain bytes between control words; line breaks in the file are dropped.
    Text(Vec<u8>),
    /// `\binN` raw bytes.
    Bin(Vec<u8>),
}

impl Node {
    /// The group's destination: `\*\name` or a leading `\name`.
    pub fn destination(children: &[Node]) -> Option<(&str, bool)> {
        match children {
            [Node::Symbol('*'), Node::Word(name, _), ..] => Some((name, true)),
            [Node::Word(name, _), ..] => Some((name, false)),
            _ => None,
        }
    }
}

/// The parsed tree (the children of an implicit root) and whether braces were unbalanced
/// (missing closers, extra closers).
pub fn parse(bytes: &[u8]) -> (Vec<Node>, usize, usize) {
    let mut stack: Vec<Vec<Node>> = vec![Vec::new()];
    let mut extra_close = 0;
    let mut i = 0;
    let push = |stack: &mut Vec<Vec<Node>>, n: Node| stack.last_mut().expect("root").push(n);
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                stack.push(Vec::new());
                i += 1;
            }
            b'}' => {
                if stack.len() > 1 {
                    let g = stack.pop().expect("a group");
                    push(&mut stack, Node::Group(g));
                } else {
                    extra_close += 1;
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&c) = bytes.get(i) else { break };
                if c.is_ascii_alphabetic() {
                    let start = i;
                    while i < bytes.len() && bytes[i].is_ascii_alphabetic() && i - start < 32 {
                        i += 1;
                    }
                    let name = String::from_utf8_lossy(&bytes[start..i]).into_owned();
                    let num_start = i;
                    if bytes.get(i) == Some(&b'-') {
                        i += 1;
                    }
                    while i < bytes.len() && bytes[i].is_ascii_digit() && i - num_start < 11 {
                        i += 1;
                    }
                    let param = std::str::from_utf8(&bytes[num_start..i])
                        .ok()
                        .and_then(|s| s.parse::<i64>().ok())
                        .map(|n| n.clamp(i32::MIN as i64, i32::MAX as i64) as i32);
                    if param.is_none() {
                        // A lone `-` wasn't a number.
                        i = num_start;
                    }
                    if bytes.get(i) == Some(&b' ') {
                        i += 1;
                    }
                    if name == "bin" {
                        let n = param.unwrap_or(0).max(0) as usize;
                        let end = (i + n).min(bytes.len());
                        push(&mut stack, Node::Bin(bytes[i..end].to_vec()));
                        i = end;
                    } else {
                        push(&mut stack, Node::Word(name, param));
                    }
                } else if c == b'\'' {
                    let hex = bytes.get(i + 1..i + 3).and_then(|h| {
                        std::str::from_utf8(h)
                            .ok()
                            .and_then(|h| u8::from_str_radix(h, 16).ok())
                    });
                    match hex {
                        Some(b) => {
                            push(&mut stack, Node::Hex(b));
                            i += 3;
                        }
                        None => i += 1,
                    }
                } else {
                    let sym = if c == b'\r' || c == b'\n' {
                        '\n'
                    } else {
                        c as char
                    };
                    push(&mut stack, Node::Symbol(sym));
                    i += 1;
                    // `\` CR LF is one paragraph break.
                    if c == b'\r' && bytes.get(i) == Some(&b'\n') {
                        i += 1;
                    }
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                let start = i;
                while i < bytes.len() && !matches!(bytes[i], b'{' | b'}' | b'\\' | b'\r' | b'\n') {
                    i += 1;
                }
                let text = &bytes[start..i];
                match stack.last_mut().expect("root").last_mut() {
                    Some(Node::Text(t)) => t.extend_from_slice(text),
                    _ => push(&mut stack, Node::Text(text.to_vec())),
                }
            }
        }
    }
    let missing_close = stack.len() - 1;
    while stack.len() > 1 {
        let g = stack.pop().expect("a group");
        push(&mut stack, Node::Group(g));
    }
    (stack.pop().expect("root"), missing_close, extra_close)
}

#[cfg(test)]
mod tests {
    use super::{Node, parse};

    #[test]
    fn tokens_and_groups() {
        let (tree, missing, extra) =
            parse(b"{\\rtf1\\ansi {\\b bold\\b0} \\'e9\\u-3913 ?\\~x\\\n}");
        assert_eq!((missing, extra), (0, 0));
        let Node::Group(g) = &tree[0] else { panic!() };
        assert_eq!(g[0], Node::Word("rtf".into(), Some(1)));
        assert_eq!(g[1], Node::Word("ansi".into(), None));
        assert_eq!(
            g[2],
            Node::Group(vec![
                Node::Word("b".into(), None),
                Node::Text(b"bold".to_vec()),
                Node::Word("b".into(), Some(0)),
            ])
        );
        assert_eq!(g[3], Node::Text(b" ".to_vec()));
        assert_eq!(g[4], Node::Hex(0xe9));
        assert_eq!(g[5], Node::Word("u".into(), Some(-3913)));
        assert_eq!(g[6], Node::Text(b"?".to_vec()));
        assert_eq!(g[7], Node::Symbol('~'));
        assert_eq!(g[8], Node::Text(b"x".to_vec()));
        assert_eq!(g[9], Node::Symbol('\n'));
    }

    #[test]
    fn bin_reads_raw_bytes_and_braces_are_counted() {
        let (tree, missing, extra) = parse(b"{\\pict\\bin3 {}\\x}}{\\b");
        assert_eq!((missing, extra), (1, 1));
        let Node::Group(g) = &tree[0] else { panic!() };
        assert_eq!(g[1], Node::Bin(b"{}\\".to_vec()));
    }
}
