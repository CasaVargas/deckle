// SPDX-License-Identifier: GPL-3.0-or-later
//! `cargo run -p deckle-pdf --example reflow -- in.pdf out.epub`: one PDF through the importer,
//! with its report, for trying the layout passes on real documents.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [pdf, epub] = args.as_slice() else {
        eprintln!("usage: reflow <in.pdf> <out.epub>");
        std::process::exit(2);
    };
    match deckle_pdf::convert(pdf.as_ref(), epub.as_ref()) {
        Ok(r) => println!("{r:#?}"),
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    }
}
