// SPDX-License-Identifier: GPL-3.0-or-later
//! Prints the decoded positions of each `.yjf` given: `cargo run -p deckle-readership --example yjf -- <file.yjf>…`

fn main() {
    for path in std::env::args().skip(1) {
        match std::fs::read(&path).map(|b| deckle_readership::parse_yjf(&b)) {
            Ok(Ok(p)) => println!("{path}: {p:?}"),
            Ok(Err(e)) => println!("{path}: {e}"),
            Err(e) => println!("{path}: {e}"),
        }
    }
}
