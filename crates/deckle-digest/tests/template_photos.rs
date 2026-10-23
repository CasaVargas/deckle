//! A site's template picture (default author avatar) must not print inside stories.

use deckle_digest::article::{eink_photo, fetch_photos};
use deckle_digest::sources::feeds::photo_token;
use deckle_digest::{Article, Edition, Section};

fn png(w: u32, h: u32, seed: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        // blocky pseudo-random texture, different per seed, same at any size
        let (bx, by) = (x * 12 / w, y * 12 / h);
        let v =
            ((bx * 7919 + by * 104_729 + seed * 1_299_709).wrapping_mul(2_654_435_761) >> 24) as u8;
        image::Rgb([v, v / 3, 255 - v])
    });
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn story(photos: Vec<Vec<u8>>, lead: bool) -> Article {
    let mut body = String::new();
    for n in 0..photos.len() {
        if n == 0 && lead {
            body.push_str(&format!(
                "<div class=\"figure lead\"><img src=\"{}\" alt=\"\"/></div>\n",
                photo_token(n)
            ));
        } else {
            body.push_str(&format!(
                "<div class=\"figure\"><img src=\"{}\" alt=\"\"/></div>\n<p>text {n}</p>",
                photo_token(n)
            ));
        }
    }
    Article {
        title: "t".into(),
        body,
        photos,
        ..Default::default()
    }
}

fn edition(articles: Vec<Article>) -> Edition {
    Edition {
        paper: "P".into(),
        date: chrono::NaiveDate::from_ymd_opt(2026, 10, 22).unwrap(),
        place: String::new(),
        weather_line: None,
        sections: vec![Section {
            articles,
            ..Default::default()
        }],
        assets: Vec::new(),
    }
}

#[test]
fn a_277px_avatar_repeated_across_stories_is_dropped_and_real_photos_stay() {
    let avatar = eink_photo(&png(277, 277, 7)).unwrap();
    let lead1 = eink_photo(&png(800, 500, 1)).unwrap();
    let lead2 = eink_photo(&png(800, 500, 2)).unwrap();
    let inline = eink_photo(&png(700, 450, 3)).unwrap();
    let mut e = edition(vec![
        story(vec![lead1.clone(), avatar.clone(), inline.clone()], true),
        story(vec![lead2.clone(), avatar.clone()], true),
    ]);
    e.drop_template_photos();
    let (a, b) = (&e.sections[0].articles[0], &e.sections[0].articles[1]);
    assert_eq!(a.photos, vec![lead1, inline]);
    assert_eq!(b.photos, vec![lead2]);
    // The inline photo moved from slot 2 to slot 1; nothing points past the list.
    assert!(a.body.contains(&photo_token(1)) && !a.body.contains(&photo_token(2)));
    assert!(!b.body.contains(&photo_token(1)) && !b.body.contains("<div class=\"figure\">"));
}

#[test]
fn photos_repeated_only_within_one_story_and_lead_photos_are_untouched() {
    let shared = eink_photo(&png(800, 500, 4)).unwrap();
    let mut e = edition(vec![
        story(vec![shared.clone(), shared.clone()], true),
        story(vec![shared.clone()], true),
    ]);
    // Same lead in two stories (a wire photo): leads stay. The inline repeat in story one
    // appears only in that story, but its bytes are the other story's lead too: it goes.
    e.drop_template_photos();
    assert_eq!(e.sections[0].articles[1].photos.len(), 1);
    assert_eq!(e.sections[0].articles[0].photos.len(), 1);
    let solo = eink_photo(&png(800, 500, 5)).unwrap();
    let mut e = edition(vec![story(vec![solo.clone(), solo.clone()], true)]);
    e.drop_template_photos();
    assert_eq!(e.sections[0].articles[0].photos.len(), 2);
}

#[test]
fn a_small_inline_image_is_dropped_but_a_small_lead_and_big_inline_stay() {
    let (small, lead, big) = (png(277, 277, 7), png(250, 250, 8), png(640, 400, 9));
    let net = move |u: &str| -> Result<Vec<u8>, String> {
        Ok(match u {
            "https://x/lead.png" => lead.clone(),
            "https://x/small.png" => small.clone(),
            _ => big.clone(),
        })
    };
    let urls: Vec<String> = ["lead", "small", "big"]
        .iter()
        .map(|n| format!("https://x/{n}.png"))
        .collect();
    let got = fetch_photos(&urls, true, 9, &net);
    assert!(got[0].is_some(), "lead clears the 200 px floor");
    assert!(got[1].is_none(), "277 px square inline is an avatar");
    assert!(got[2].is_some());
}
