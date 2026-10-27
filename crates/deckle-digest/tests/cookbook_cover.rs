// SPDX-License-Identifier: GPL-3.0-or-later
//! The cookbook's cover (cookbook design §6.3): the count spelled out, the title wrapped,
//! the last recipe added at the foot.

use deckle_digest::cookbook::{count_words, subtitle_line};
use deckle_digest::cover::render_cookbook;

fn decode(bytes: &[u8]) -> image::GrayImage {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .expect("a JPEG")
        .to_luma8()
}

fn dark_pixels(img: &image::GrayImage, y0: u32, y1: u32) -> usize {
    img.enumerate_pixels()
        .filter(|(_, y, p)| *y >= y0 && *y < y1 && p.0[0] < 128)
        .count()
}

#[test]
fn cover_is_1600x2560_jpeg() {
    let bytes = render_cookbook(
        "The Vargas Kitchen Book",
        "Recipes, gathered",
        47,
        Some("Roasted squash soup"),
    )
    .expect("renders");
    assert_eq!(&bytes[..2], &[0xFF, 0xD8], "JPEG magic");
    let img = decode(&bytes);
    assert_eq!(img.dimensions(), (1600, 2560));
    // The ornament sits near the top, the title in the middle, the foot at the bottom.
    assert!(dark_pixels(&img, 100, 800) > 2_000, "ornament");
    assert!(dark_pixels(&img, 900, 1700) > 2_000, "title");
    assert!(dark_pixels(&img, 2300, 2560) > 500, "foot");
}

#[test]
fn the_cover_is_the_same_every_time() {
    let a = render_cookbook("Soup", "Recipes, gathered", 3, Some("Pea")).unwrap();
    let b = render_cookbook("Soup", "Recipes, gathered", 3, Some("Pea")).unwrap();
    assert_eq!(a, b, "the ornament is fixed, not generated");
    let c = render_cookbook("Soup", "Recipes, gathered", 4, Some("Pea")).unwrap();
    assert_ne!(a, c, "the count is on the cover");
}

#[test]
fn an_empty_book_has_no_foot_line() {
    let bytes = render_cookbook("Soup", "Recipes, gathered", 0, None).expect("renders");
    let img = decode(&bytes);
    assert_eq!(img.dimensions(), (1600, 2560));
    assert_eq!(dark_pixels(&img, 2400, 2560), 0);
}

#[test]
fn count_words_table() {
    for (n, words) in [
        (0, "No"),
        (1, "One"),
        (7, "Seven"),
        (12, "Twelve"),
        (19, "Nineteen"),
        (20, "Twenty"),
        (21, "Twenty-one"),
        (47, "Forty-seven"),
        (99, "Ninety-nine"),
        (100, "One hundred"),
        (101, "One hundred one"),
        (342, "Three hundred forty-two"),
        (999, "Nine hundred ninety-nine"),
    ] {
        assert_eq!(count_words(n), words, "{n}");
    }
    assert_eq!(count_words(1_000), "1,000");
    assert_eq!(count_words(12_345), "12,345");
}

#[test]
fn subtitle_line_singular_and_plural() {
    assert_eq!(
        subtitle_line("Recipes, gathered", 47),
        "Forty-seven recipes, gathered"
    );
    assert_eq!(
        subtitle_line("Recipes, gathered", 1),
        "One recipe, gathered"
    );
    assert_eq!(
        subtitle_line("Recipes, gathered", 0),
        "No recipes, gathered"
    );
    assert_eq!(
        subtitle_line("recipes, gathered", 2),
        "Two recipes, gathered"
    );
    // Another subtitle: the count words go in front, its first letter lowercased.
    assert_eq!(
        subtitle_line("Dishes from the family", 12),
        "Twelve dishes from the family"
    );
    assert_eq!(
        subtitle_line("Recipes, gathered", 1_500),
        "1,500 recipes, gathered"
    );
}

#[test]
fn long_title_wraps_without_panic() {
    let title = "The Complete and Entirely Unabridged Vargas Family Kitchen Book of Soups, \
                 Stews, Roasts, Breads and Every Other Dish Worth Writing Down"
        .repeat(3);
    let bytes = render_cookbook(&title, "Recipes, gathered", 47, Some(&title)).expect("renders");
    assert_eq!(decode(&bytes).dimensions(), (1600, 2560));
    // One unbreakable word wider than the page, and an empty title.
    let word = "Supercalifragilisticexpialidocious".repeat(4);
    render_cookbook(&word, "Recipes, gathered", 1, Some(&word)).expect("renders");
    render_cookbook("", "", 0, None).expect("renders");
}
