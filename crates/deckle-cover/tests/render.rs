// SPDX-License-Identifier: GPL-3.0-or-later
//! Typographic covers (cover design §4): size, determinism, the template hash, scripts the
//! bundled fonts can't draw, and golden images per template.

use deckle_cover::{CoverText, HEIGHT, Template, WIDTH, render, render_image, series_line};
use image::GrayImage;

fn text(title: &str, authors: &[&str], series: Option<&str>) -> CoverText {
    CoverText {
        title: title.into(),
        authors: authors.iter().map(|a| (*a).to_owned()).collect(),
        series: series.map(str::to_owned),
    }
}

fn samples() -> Vec<CoverText> {
    vec![
        text("Dune", &["Frank Herbert"], None),
        text(
            "The Remarkable and Entirely Unabridged Account of a Voyage Around the Known World \
             in Eighty Days, with Notes, Maps and an Appendix",
            &["Jane Testwright"],
            None,
        ),
        text(
            "Thinking in Systems: A Primer for People Who Like to Understand",
            &["Donella H. Meadows"],
            None,
        ),
        text("Good Omens", &["Terry Pratchett", "Neil Gaiman"], None),
        text(
            "Freakonomics",
            &["Steven D. Levitt", "Stephen J. Dubner", "A. N. Other"],
            None,
        ),
        text(
            "The Dispossessed",
            &["Ursula K. Le Guin"],
            Some("Hainish Cycle · 5"),
        ),
    ]
}

#[test]
fn every_template_renders_a_full_size_grayscale_jpeg() {
    for t in Template::ALL {
        for s in samples() {
            let cover = render(&s, t).unwrap();
            assert!(
                cover.warnings.is_empty(),
                "{t:?} {s:?}: {:?}",
                cover.warnings
            );
            assert_eq!(&cover.jpeg[..2], &[0xFF, 0xD8], "{t:?} is a JPEG");
            let img = image::load_from_memory(&cover.jpeg).unwrap();
            assert_eq!((img.width(), img.height()), (WIDTH, HEIGHT));
            assert_eq!(img.color(), image::ColorType::L8, "{t:?} is grayscale");
        }
    }
}

#[test]
fn rendering_is_byte_identical_for_the_same_input() {
    for t in Template::ALL {
        let s = &samples()[5];
        assert_eq!(render(s, t).unwrap().jpeg, render(s, t).unwrap().jpeg);
    }
}

#[test]
fn the_template_chosen_by_hash_is_stable_and_varied() {
    // FNV-1a over the identifier: pinned so a shelf keeps its covers across releases.
    assert_eq!(
        Template::for_book("urn:uuid:1"),
        Template::for_book("urn:uuid:1")
    );
    let picked: std::collections::BTreeSet<_> = (0..30)
        .map(|i| Template::for_book(&format!("urn:uuid:{i}")).name())
        .collect();
    assert_eq!(picked.len(), 3, "{picked:?}");
    assert_eq!(Template::for_book("").name(), "rule");
}

#[test]
fn templates_parse_by_name() {
    for t in Template::ALL {
        assert_eq!(t.name().parse::<Template>(), Ok(t));
    }
    assert!("fancy".parse::<Template>().is_err());
}

#[test]
fn a_title_the_fonts_cannot_draw_is_left_off_with_a_warning() {
    for t in Template::ALL {
        let cjk = text("吾輩は猫である", &["Natsume Sōseki"], Some("Classics · 1"));
        let cover = render(&cjk, t).unwrap();
        assert_eq!(
            cover.warnings,
            vec!["cover title not renderable in the bundled fonts".to_owned()]
        );
        // The author and series still render: the cover isn't blank.
        let (img, _) = render_image(&cjk, t).unwrap();
        let blank = render_image(&text("吾輩は猫である", &[], None), t)
            .unwrap()
            .0;
        assert_ne!(img, blank, "{t:?} still draws the author");
    }
}

#[test]
fn cyrillic_titles_render_in_every_template_and_greek_in_inter() {
    for t in Template::ALL {
        let cover = render(&text("Война и мир", &["Лев Толстой"], None), t).unwrap();
        assert!(cover.warnings.is_empty(), "{:?}", cover.warnings);
    }
    let greek = text("Ὀδύσσεια", &["Ὅμηρος"], None);
    for t in [Template::Band, Template::Rule] {
        assert!(render(&greek, t).unwrap().warnings.is_empty());
    }
}

#[test]
fn the_hashed_choice_skips_a_template_that_cant_set_the_title() {
    let greek = text("Ὀδύσσεια", &["Ὅμηρος"], None);
    let latin = text("Dune", &[], None);
    let classic_id = (0..)
        .map(|i| format!("urn:uuid:{i}"))
        .find(|id| Template::for_book(id) == Template::Classic)
        .unwrap();
    assert_eq!(Template::choose(&classic_id, &latin), Template::Classic);
    assert_eq!(Template::choose(&classic_id, &greek), Template::Band);
    // Nothing sets CJK: the hashed template stands, and the title is left off.
    let cjk = text("吾輩は猫である", &[], None);
    assert_eq!(Template::choose(&classic_id, &cjk), Template::Classic);
}

#[test]
fn series_lines_read_like_a_shelf() {
    assert_eq!(series_line("Hainish Cycle", Some(5.0)), "Hainish Cycle · 5");
    assert_eq!(series_line("Discworld", Some(1.5)), "Discworld · 1.5");
    assert_eq!(series_line("Discworld", None), "Discworld");
}

/// Pixels further apart than this count as different...
const PIXEL_TOLERANCE: u8 = 48;
/// ...and up to this share of them may differ (a rasteriser update moves edge pixels).
const MAX_DIFFERING: f64 = 0.002;

#[test]
fn templates_match_their_golden_images() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let s = text(
        "The Left Hand of Darkness: A Novel",
        &["Ursula K. Le Guin"],
        Some("Hainish Cycle · 4"),
    );
    for t in Template::ALL {
        let (img, _) = render_image(&s, t).unwrap();
        let path = dir.join(format!("{}.png", t.name()));
        if std::env::var_os("DECKLE_BLESS").is_some() {
            img.save(&path).unwrap();
            continue;
        }
        let golden: GrayImage = image::open(&path)
            .unwrap_or_else(|e| panic!("{}: {e} (DECKLE_BLESS=1 writes it)", path.display()))
            .to_luma8();
        assert_eq!(golden.dimensions(), img.dimensions());
        let differing = golden
            .pixels()
            .zip(img.pixels())
            .filter(|(a, b)| a.0[0].abs_diff(b.0[0]) > PIXEL_TOLERANCE)
            .count();
        let share = differing as f64 / f64::from(WIDTH * HEIGHT);
        assert!(
            share <= MAX_DIFFERING,
            "{t:?}: {differing} pixels differ from {}",
            path.display()
        );
    }
}
