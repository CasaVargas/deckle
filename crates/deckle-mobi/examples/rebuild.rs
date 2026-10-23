// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo run -p deckle-mobi --example rebuild -- book.azw3 out.epub`: a KF8 book back into an
//! EPUB, for trying the rebuild on real files.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        eprintln!("usage: rebuild <book.azw3> <out.epub>");
        std::process::exit(2);
    };
    let bytes = std::fs::read(input).expect("read the book");
    match deckle_mobi::kf8::rebuild(&bytes, std::path::Path::new(output)) {
        Ok(r) => {
            println!("{}: {} parts, {} images", r.title, r.parts, r.images);
            for w in r.warnings {
                println!("warning: {w}");
            }
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
