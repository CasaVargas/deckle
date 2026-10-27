// SPDX-License-Identifier: GPL-3.0-or-later
//! The cookbook EPUB (cookbook design §6.1, §6.2, §6.4): one book, a fixed identifier, chapters
//! in config order, one recipe per section, the contents and the index linking to them.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use chrono::{DateTime, TimeZone, Utc};
use deckle_digest::cookbook::book::{BOOK_UID, DEVICE_NAME, build, printed_picture};
use deckle_digest::cookbook::store::{BookConfig, ImageKind, Picture, Saved, Store};
use deckle_digest::cookbook::{Group, Recipe};
use roxmltree::{Document, Node};

fn at(m: u32, d: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, m, d, 12, 0, 0).unwrap()
}

fn now() -> DateTime<Utc> {
    at(10, 27)
}

fn blank(title: &str) -> Recipe {
    Recipe {
        title: title.to_owned(),
        description: None,
        yield_: None,
        prep: None,
        cook: None,
        total: None,
        ingredients: vec![],
        steps: vec![],
        category: vec![],
        cuisine: None,
        author: None,
        image_url: None,
        source_url: String::new(),
        site_name: String::new(),
        structured: true,
        body_xhtml: None,
    }
}

fn group(heading: Option<&str>, items: &[&str]) -> Group {
    Group {
        heading: heading.map(str::to_owned),
        items: items.iter().map(|s| s.to_string()).collect(),
    }
}

fn jpeg() -> Vec<u8> {
    let img = image::GrayImage::from_pixel(60, 40, image::Luma([120]));
    let mut out = Vec::new();
    image::DynamicImage::ImageLuma8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut out),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    out
}

fn put_image(store: &Store, id: &str, kind: ImageKind) {
    let path = store.image_path(id, kind);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, jpeg()).unwrap();
}

/// Adds a recipe to the store (filed in `chapter`, saved `added`) and returns it as saved.
fn save(store: &Store, recipe: Recipe, chapter: &str, added: DateTime<Utc>) -> Saved {
    let key = format!("https://kitchen.test/{}", recipe.title.replace(' ', "-"));
    store.put(recipe, &key, Some(chapter), None, added).unwrap();
    store.find(&key).unwrap().unwrap()
}

struct Fixture {
    _dir: tempfile::TempDir,
    store: Store,
    soup: Saved,
    chili: Saved,
    bread: Saved,
    loaf: Saved,
    roast: Saved,
    mystery: Saved,
}

/// Six recipes: two soups, two bakes, an unstructured roast, and one filed in a chapter that has
/// since been deleted.
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("cookbook"));

    let mut r = blank("Roasted Squash Soup");
    r.description = Some("A velvety soup for cold nights.".to_owned());
    r.yield_ = Some("4 servings".to_owned());
    r.prep = Some(15);
    r.cook = Some(50);
    r.cuisine = Some("Thai".to_owned());
    r.author = Some("Ada Cook".to_owned());
    r.site_name = "Kitchen Test".to_owned();
    r.source_url = "https://kitchen.test/squash-soup".to_owned();
    r.ingredients = vec![
        group(None, &["1 butternut squash", "2 tbsp olive oil"]),
        group(Some("For the garnish"), &["1/4 cup pumpkin seeds"]),
    ];
    r.steps = vec![
        group(Some("The soup"), &["Roast the squash.", "Blend it."]),
        group(Some("To serve"), &["Scatter the seeds."]),
    ];
    let mut soup = save(&store, r, "soups", at(10, 25));
    soup.note = Some("Less ginger next time.".to_owned());
    store.update(&soup).unwrap();
    put_image(&store, &soup.id, ImageKind::Printed);

    let mut r = blank("bean chili");
    r.total = Some(30);
    r.ingredients = vec![group(None, &["1 can black beans", "1 onion"])];
    r.steps = vec![group(None, &["Simmer everything."])];
    let chili = save(&store, r, "soups", at(9, 1));

    let mut r = blank("Zucchini Bread");
    r.ingredients = vec![group(None, &["2 zucchini", "2 cups flour"])];
    r.steps = vec![group(None, &["Bake."])];
    let bread = save(&store, r, "baking", at(10, 1));
    put_image(&store, &bread.id, ImageKind::Printed);

    let mut r = blank("apple Loaf");
    r.ingredients = vec![group(None, &["3 apples", "2 cups flour"])];
    r.steps = vec![group(None, &["Bake it longer."])];
    let mut loaf = save(&store, r, "baking", at(10, 2));
    loaf.picture = Picture::Illustration;
    store.update(&loaf).unwrap();
    put_image(&store, &loaf.id, ImageKind::Ill);
    put_image(&store, &loaf.id, ImageKind::Printed);

    let mut r = blank("Pot Roast");
    r.structured = false;
    r.source_url = "https://kitchen.test/roast".to_owned();
    r.site_name = "Kitchen Test".to_owned();
    r.body_xhtml = Some(
        "<div class=\"figure lead\"><img src=\"__DECKLE_IMG_0__\" alt=\"\"/></div>\n\
         <p>Brown the roast in a heavy pot.</p>\n\
         <div class=\"figure\"><img src=\"__DECKLE_IMG_1__\" alt=\"\"/><span class=\"caption\">The pot</span></div>\n\
         <p>Braise for three hours.<img src=\"__DECKLE_IMG_2__\" alt=\"\"/></p>\n\
         <figure><img src=\"__DECKLE_IMG_3__\"/><figcaption>Done</figcaption></figure>"
            .to_owned(),
    );
    let roast = save(&store, r, "mains", at(10, 10));
    put_image(&store, &roast.id, ImageKind::Printed);

    let mut r = blank("Mystery <Dish> & Co");
    r.source_url = "https://kitchen.test/mystery?a=1&b=<2>".to_owned();
    r.site_name = "Kitchen <Test>".to_owned();
    r.ingredients = vec![group(None, &["1 egg & <more>"])];
    r.steps = vec![group(None, &["Whisk."])];
    let mystery = save(&store, r, "gone", at(10, 21));

    Fixture {
        _dir: dir,
        store,
        soup,
        chili,
        bread,
        loaf,
        roast,
        mystery,
    }
}

type Entries = BTreeMap<String, Vec<u8>>;

fn built(
    store: &Store,
    now: DateTime<Utc>,
) -> (tempfile::TempDir, deckle_digest::cookbook::book::Built) {
    let out = tempfile::tempdir().unwrap();
    let b = build(store, now, &out.path().join("book.epub")).unwrap();
    (out, b)
}

fn entries(path: &Path) -> Entries {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut out = BTreeMap::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).unwrap();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).unwrap();
        out.insert(f.name().unwrap().into_owned(), bytes);
    }
    out
}

fn text<'a>(e: &'a Entries, name: &str) -> &'a str {
    std::str::from_utf8(
        e.get(name)
            .unwrap_or_else(|| panic!("no {name} in {:?}", e.keys())),
    )
    .unwrap()
}

fn epub(store: &Store, now: DateTime<Utc>) -> (Entries, deckle_digest::cookbook::book::Built) {
    let (_out, b) = built(store, now);
    (entries(&b.path), b)
}

fn xml<'a>(src: &'a str) -> Document<'a> {
    Document::parse(src).unwrap_or_else(|e| panic!("not well-formed: {e}\n{src}"))
}

fn has_class(n: Node, class: &str) -> bool {
    n.attribute("class")
        .is_some_and(|c| c.split_whitespace().any(|x| x == class))
}

fn all_text(n: Node) -> String {
    n.descendants()
        .filter(|d| d.is_text())
        .map(|d| d.text().unwrap())
        .collect()
}

fn by_class<'a, 'i>(doc: &'a Document<'i>, tag: &str, class: &str) -> Vec<Node<'a, 'i>> {
    doc.descendants()
        .filter(|n| n.has_tag_name(tag) && has_class(*n, class))
        .collect()
}

fn recipe_section<'a, 'i>(doc: &'a Document<'i>, id: &str) -> Node<'a, 'i> {
    doc.descendants()
        .find(|n| n.has_tag_name("section") && n.attribute("id") == Some(&format!("r-{id}")))
        .unwrap_or_else(|| panic!("no section r-{id}"))
}

fn recipe_ids(doc: &Document) -> Vec<String> {
    doc.descendants()
        .filter(|n| n.has_tag_name("section") && has_class(*n, "recipe"))
        .map(|n| {
            n.attribute("id")
                .unwrap()
                .trim_start_matches("r-")
                .to_owned()
        })
        .collect()
}

/// The spine's file names, in reading order.
fn spine(e: &Entries) -> Vec<String> {
    let opf = text(e, "OEBPS/content.opf");
    let doc = xml(opf);
    let manifest: BTreeMap<_, _> = doc
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .map(|n| (n.attribute("id").unwrap(), n.attribute("href").unwrap()))
        .collect();
    doc.descendants()
        .filter(|n| n.has_tag_name("itemref"))
        .map(|n| manifest[n.attribute("idref").unwrap()].to_owned())
        .collect()
}

#[test]
fn identifier_is_fixed() {
    assert_eq!(BOOK_UID, "urn:deckle:cookbook:default");
    assert_eq!(DEVICE_NAME, "Deckle Kitchen Book.kfx");
    let f = fixture();
    let (e, b) = epub(&f.store, now());
    let opf = text(&e, "OEBPS/content.opf");
    assert!(
        opf.contains("<dc:identifier id=\"uid\">urn:deckle:cookbook:default</dc:identifier>"),
        "{opf}"
    );
    assert!(
        opf.contains("<dc:title>The Kitchen Book</dc:title>"),
        "{opf}"
    );
    assert_eq!(b.title, "The Kitchen Book");
    assert_eq!(b.recipes, 6);
    // Renaming the book changes the title, not the identifier.
    let mut cfg = f.store.config().unwrap();
    cfg.title = "Vargas Kitchen".to_owned();
    f.store.save_config(&cfg).unwrap();
    let (e, b) = epub(&f.store, now());
    let opf = text(&e, "OEBPS/content.opf");
    assert!(opf.contains("urn:deckle:cookbook:default"), "{opf}");
    assert!(opf.contains("<dc:title>Vargas Kitchen</dc:title>"), "{opf}");
    assert_eq!(b.title, "Vargas Kitchen");
}

#[test]
fn chapters_in_config_order_empty_omitted() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let chapters: Vec<String> = spine(&e)
        .into_iter()
        .filter(|h| h.starts_with("chapter-"))
        .collect();
    // Default order: soups, mains, baking, other. Breakfast, salads and the rest are empty.
    assert_eq!(
        chapters,
        [
            "chapter-soups.xhtml",
            "chapter-mains.xhtml",
            "chapter-baking.xhtml",
            "chapter-other.xhtml"
        ]
    );
    let soups = text(&e, "OEBPS/chapter-soups.xhtml");
    let doc = xml(soups);
    let h1 = doc.descendants().find(|n| n.has_tag_name("h1")).unwrap();
    assert_eq!(all_text(h1), "Soups and stews");
    // The chapter title page comes before its first recipe.
    let order: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("section"))
        .map(|n| n.attribute("class").unwrap_or("").to_owned())
        .collect();
    assert_eq!(order[0], "chapter-title");
    // Empty chapters stay out of the nav.
    let nav = text(&e, "OEBPS/nav.xhtml");
    assert!(!nav.contains("Breakfast"), "{nav}");
}

#[test]
fn recipes_alphabetical_in_chapter() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let baking = xml(text(&e, "OEBPS/chapter-baking.xhtml"));
    // Case-insensitive: "apple Loaf" before "Zucchini Bread".
    assert_eq!(recipe_ids(&baking), [f.loaf.id.clone(), f.bread.id.clone()]);
    let soups = xml(text(&e, "OEBPS/chapter-soups.xhtml"));
    assert_eq!(recipe_ids(&soups), [f.chili.id.clone(), f.soup.id.clone()]);
}

#[test]
fn title_override_is_the_displayed_title() {
    let f = fixture();
    let mut s = f.chili.clone();
    s.title_override = Some("Zesty Chili".to_owned());
    f.store.update(&s).unwrap();
    let (e, _) = epub(&f.store, now());
    let soups = xml(text(&e, "OEBPS/chapter-soups.xhtml"));
    // Sorted by the displayed title: "Roasted Squash Soup" < "Zesty Chili".
    assert_eq!(recipe_ids(&soups), [f.soup.id.clone(), f.chili.id.clone()]);
    let sec = recipe_section(&soups, &f.chili.id);
    let h2 = sec.descendants().find(|n| n.has_tag_name("h2")).unwrap();
    assert_eq!(all_text(h2), "Zesty Chili");
    let contents = text(&e, "OEBPS/contents.xhtml");
    assert!(contents.contains("Zesty Chili"), "{contents}");
    assert!(!contents.contains("bean chili"), "{contents}");
}

#[test]
fn deleted_chapter_prints_under_everything_else() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let other = xml(text(&e, "OEBPS/chapter-other.xhtml"));
    assert_eq!(recipe_ids(&other), [f.mystery.id.as_str()]);
    let h1 = other.descendants().find(|n| n.has_tag_name("h1")).unwrap();
    assert_eq!(all_text(h1), "Everything else");
    // The recipe file was not rewritten.
    assert_eq!(
        f.store.find(&f.mystery.id).unwrap().unwrap().chapter,
        "gone"
    );
    // And no chapter-gone file.
    assert!(!e.keys().any(|k| k.contains("gone")));
}

#[test]
fn star_on_recipes_newer_than_7_days() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let doc = xml(text(&e, "OEBPS/contents.xhtml"));
    let entry = |id: &str| -> String {
        let a = doc
            .descendants()
            .find(|n| {
                n.has_tag_name("a")
                    && n.attribute("href")
                        .is_some_and(|h| h.ends_with(&format!("#r-{id}")))
            })
            .unwrap_or_else(|| panic!("no contents link for {id}"));
        all_text(a.parent().unwrap())
    };
    // Squash soup (Oct 25) and the mystery dish (Oct 21) are within 7 days of Oct 27;
    // the chili (Sep 1) and the bread (Oct 1) are not.
    assert!(entry(&f.soup.id).contains('★'), "{}", entry(&f.soup.id));
    assert!(entry(&f.mystery.id).contains('★'));
    assert!(!entry(&f.chili.id).contains('★'));
    assert!(!entry(&f.bread.id).contains('★'));
    // Eight days on, the soup has lost its star.
    let (e, _) = epub(&f.store, at(11, 2));
    let doc = xml(text(&e, "OEBPS/contents.xhtml"));
    assert!(!all_text(doc.root_element()).contains('★'));
}

#[test]
fn contents_counts_and_new_this_week_line() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let contents = text(&e, "OEBPS/contents.xhtml");
    let doc = xml(contents);
    let flat = all_text(doc.root_element());
    assert!(flat.contains("Soups and stews"), "{flat}");
    assert!(flat.contains("2 recipes"), "{flat}");
    assert!(flat.contains("Mains"), "{flat}");
    assert!(flat.contains("1 recipe"), "{flat}");
    assert!(flat.contains("Index by ingredient"), "{flat}");
    // Soup (Oct 25) and mystery (Oct 21) are new this week.
    assert!(flat.contains("New this week: 2"), "{flat}");
    // The chapters link to their files, the index to its page.
    let hrefs: Vec<_> = doc
        .descendants()
        .filter_map(|n| n.attribute("href"))
        .collect();
    assert!(hrefs.contains(&"chapter-soups.xhtml"), "{hrefs:?}");
    assert!(hrefs.contains(&"index.xhtml"), "{hrefs:?}");
    // Recipes are under their chapter, alphabetical.
    let first_soup = flat.find("bean chili").unwrap();
    assert!(first_soup < flat.find("Roasted Squash Soup").unwrap());
}

#[test]
fn recipe_page_sections() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let doc = xml(text(&e, "OEBPS/chapter-soups.xhtml"));
    let sec = recipe_section(&doc, &f.soup.id);
    let kids: Vec<Node> = sec.children().filter(|n| n.is_element()).collect();
    // The picture first, full text width.
    let fig = kids[0];
    assert!(has_class(fig, "figure"), "{:?}", fig);
    let img = fig.descendants().find(|n| n.has_tag_name("img")).unwrap();
    assert_eq!(
        img.attribute("src"),
        Some(format!("images/{}.jpg", f.soup.id).as_str())
    );
    assert!(e.contains_key(&format!("OEBPS/images/{}.jpg", f.soup.id)));
    // Then the title, the description in italics, the facts.
    assert!(kids[1].has_tag_name("h2"));
    assert_eq!(all_text(kids[1]), "Roasted Squash Soup");
    assert!(has_class(kids[2], "description"));
    let em = kids[2]
        .descendants()
        .find(|n| n.has_tag_name("em"))
        .unwrap();
    assert_eq!(all_text(em), "A velvety soup for cold nights.");
    assert!(has_class(kids[3], "facts"));
    assert_eq!(
        all_text(kids[3]),
        "Serves 4 · Prep 15 min · Cook 50 min · Thai"
    );
    // Ingredients: a heading-less group, then a named one.
    let h3s: Vec<_> = sec
        .descendants()
        .filter(|n| n.has_tag_name("h3"))
        .map(all_text)
        .collect();
    assert_eq!(h3s, ["Ingredients", "Method"]);
    let uls: Vec<_> = sec.descendants().filter(|n| n.has_tag_name("ul")).collect();
    assert_eq!(uls.len(), 2);
    let items = |n: Node| -> Vec<String> {
        n.children()
            .filter(|c| c.has_tag_name("li"))
            .map(all_text)
            .collect()
    };
    assert_eq!(items(uls[0]), ["1 butternut squash", "2 tbsp olive oil"]);
    assert_eq!(items(uls[1]), ["1/4 cup pumpkin seeds"]);
    let h4s: Vec<_> = sec
        .descendants()
        .filter(|n| n.has_tag_name("h4"))
        .map(all_text)
        .collect();
    assert_eq!(h4s, ["For the garnish", "The soup", "To serve"]);
    // Method: numbered lists with sub-headings.
    let ols: Vec<_> = sec.descendants().filter(|n| n.has_tag_name("ol")).collect();
    assert_eq!(ols.len(), 2);
    assert_eq!(items(ols[0]), ["Roast the squash.", "Blend it."]);
    assert_eq!(items(ols[1]), ["Scatter the seeds."]);
    // The note, set off in a box.
    let note = by_class(&doc, "div", "note");
    assert_eq!(note.len(), 1);
    assert!(all_text(note[0]).contains("Your note"));
    assert!(all_text(note[0]).contains("Less ginger next time."));
    // The foot: site, author, date and the URL as text, not a link.
    let foot = sec
        .descendants()
        .find(|n| has_class(*n, "foot"))
        .expect("a foot");
    let foot_text = all_text(foot);
    assert!(
        foot_text.contains("From Kitchen Test · by Ada Cook · saved Oct 25, 2026"),
        "{foot_text}"
    );
    assert!(foot_text.contains("https://kitchen.test/squash-soup"));
    assert!(!foot.descendants().any(|n| n.has_tag_name("a")), "no link");
    // Missing parts are left out: the chili has only a total time, no picture, note or author.
    let chili = recipe_section(&doc, &f.chili.id);
    let facts = chili
        .descendants()
        .find(|n| has_class(*n, "facts"))
        .unwrap();
    assert_eq!(all_text(facts), "Total 30 min");
    assert!(!chili.descendants().any(|n| n.has_tag_name("img")));
    assert!(!chili.descendants().any(|n| has_class(n, "description")));
    assert!(!chili.descendants().any(|n| has_class(n, "note")));
    assert!(!all_text(chili).contains(" by "));
}

#[test]
fn hostile_text_is_escaped() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let other = text(&e, "OEBPS/chapter-other.xhtml");
    let doc = xml(other);
    let sec = recipe_section(&doc, &f.mystery.id);
    let h2 = sec.descendants().find(|n| n.has_tag_name("h2")).unwrap();
    assert_eq!(all_text(h2), "Mystery <Dish> & Co");
    assert!(all_text(sec).contains("1 egg & <more>"));
    assert!(all_text(sec).contains("https://kitchen.test/mystery?a=1&b=<2>"));
    assert!(!other.contains("<Dish>"), "escaped in the markup");
}

#[test]
fn unstructured_recipe_prints_body() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    for (name, bytes) in &e {
        let hay = String::from_utf8_lossy(bytes);
        assert!(!hay.contains("__DECKLE_IMG_"), "placeholder left in {name}");
    }
    let doc = xml(text(&e, "OEBPS/chapter-mains.xhtml"));
    let sec = recipe_section(&doc, &f.roast.id);
    let body = all_text(sec);
    assert!(body.contains("Brown the roast in a heavy pot."), "{body}");
    assert!(body.contains("Braise for three hours."), "{body}");
    assert!(
        !body.contains("The pot"),
        "figure captions go with the figures"
    );
    // The stored picture only, once.
    let imgs: Vec<_> = sec
        .descendants()
        .filter(|n| n.has_tag_name("img"))
        .collect();
    assert_eq!(imgs.len(), 1);
    assert_eq!(
        imgs[0].attribute("src"),
        Some(format!("images/{}.jpg", f.roast.id).as_str())
    );
    // No ingredient or method headings for a page-text recipe.
    assert!(!sec.descendants().any(|n| n.has_tag_name("h3")));
}

#[test]
fn index_links_resolve() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let index = text(&e, "OEBPS/index.xhtml");
    let doc = xml(index);
    let flat = all_text(doc.root_element());
    assert!(flat.contains("Index by ingredient"), "{flat}");
    // Flour is in both baked goods.
    assert!(flat.contains("flour"), "{flat}");
    let links: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("a"))
        .map(|n| n.attribute("href").unwrap().to_owned())
        .collect();
    assert!(links.len() >= 8, "{links:?}");
    for href in links {
        let (file, id) = href.split_once('#').expect("a fragment");
        assert!(file.starts_with("chapter-"), "{href}");
        let target = xml(text(&e, &format!("OEBPS/{file}")));
        assert!(
            target.descendants().any(|n| n.attribute("id") == Some(id)),
            "{href} does not resolve"
        );
    }
    // The unstructured roast has no ingredient list, so it is not in the index.
    assert!(!index.contains(&f.roast.id));
    // The links read as titles; flour's entry lists both loaves, alphabetically.
    let flour = by_class(&doc, "p", "ixrow")
        .into_iter()
        .find(|n| all_text(*n).starts_with("flour"))
        .expect("a flour row");
    assert_eq!(all_text(flour), "flour apple Loaf, Zucchini Bread");
    // Every link elsewhere resolves too: contents, nav and the NCX.
    for name in ["contents.xhtml", "nav.xhtml", "toc.ncx"] {
        let src = text(&e, &format!("OEBPS/{name}"));
        let d = xml(src);
        for n in d.descendants() {
            let href = n.attribute("href").or_else(|| n.attribute("src"));
            let Some(href) = href.filter(|h| h.contains(".xhtml")) else {
                continue;
            };
            let (file, id) = match href.split_once('#') {
                Some((f, i)) => (f, Some(i)),
                None => (href, None),
            };
            let target = xml(text(&e, &format!("OEBPS/{file}")));
            if let Some(id) = id {
                assert!(
                    target.descendants().any(|n| n.attribute("id") == Some(id)),
                    "{name}: {href}"
                );
            }
        }
    }
}

#[test]
fn printed_picture_resolution() {
    let f = fixture();
    let cfg = f.store.config().unwrap();
    // The squash soup: Default, the book says Site, and its photo exists.
    assert_eq!(printed_picture(&f.store, &f.soup, &cfg), Picture::Site);
    // The loaf asked for an illustration and has one.
    assert_eq!(
        printed_picture(&f.store, &f.loaf, &cfg),
        Picture::Illustration
    );
    // No illustration file: falls back to the site photo.
    std::fs::remove_file(f.store.image_path(&f.loaf.id, ImageKind::Ill)).unwrap();
    assert_eq!(printed_picture(&f.store, &f.loaf, &cfg), Picture::Site);
    // No site photo either: no picture.
    std::fs::remove_file(f.store.image_path(&f.loaf.id, ImageKind::Printed)).unwrap();
    assert_eq!(printed_picture(&f.store, &f.loaf, &cfg), Picture::None);
    // Site without a photo is none, too (the chili never had one).
    assert_eq!(printed_picture(&f.store, &f.chili, &cfg), Picture::None);
    // An explicit none wins over a photo that exists.
    let mut s = f.soup.clone();
    s.picture = Picture::None;
    assert_eq!(printed_picture(&f.store, &s, &cfg), Picture::None);
    // The book's illustration setting applies to a recipe on Default.
    let mut cfg = cfg;
    cfg.picture = Picture::Illustration;
    let mut s = f.soup.clone();
    assert_eq!(
        printed_picture(&f.store, &s, &cfg),
        Picture::Site,
        "no .ill file yet"
    );
    put_image(&f.store, &s.id, ImageKind::Ill);
    assert_eq!(printed_picture(&f.store, &s, &cfg), Picture::Illustration);
    // A recipe's own Site beats the book's illustrations.
    s.picture = Picture::Site;
    assert_eq!(printed_picture(&f.store, &s, &cfg), Picture::Site);
    // The book never resolves to Default, even when the config itself says so.
    cfg.picture = Picture::Default;
    s.picture = Picture::Default;
    assert_eq!(printed_picture(&f.store, &s, &cfg), Picture::Site);
}

#[test]
fn default_picture_follows_book_setting() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    // Site is the default: the soup, bread and roast print their photos; the loaf its drawing.
    let soups = text(&e, "OEBPS/chapter-soups.xhtml");
    assert!(soups.contains(&format!("images/{}.jpg", f.soup.id)));
    let baking = text(&e, "OEBPS/chapter-baking.xhtml");
    assert!(baking.contains(&format!("images/{}.ill.jpg", f.loaf.id)));
    assert!(e.contains_key(&format!("OEBPS/images/{}.ill.jpg", f.loaf.id)));
    assert!(baking.contains(&format!("images/{}.jpg", f.bread.id)));
    // The book says none: recipes on Default print no picture, and nothing is embedded for them.
    let cfg = BookConfig {
        picture: Picture::None,
        ..f.store.config().unwrap()
    };
    f.store.save_config(&cfg).unwrap();
    let (e, _) = epub(&f.store, now());
    let soups = text(&e, "OEBPS/chapter-soups.xhtml");
    assert!(!soups.contains("<img"), "{soups}");
    assert!(!e.contains_key(&format!("OEBPS/images/{}.jpg", f.soup.id)));
    assert!(!e.contains_key(&format!("OEBPS/images/{}.jpg", f.bread.id)));
    // The loaf chose its own picture, so it keeps it.
    let baking = text(&e, "OEBPS/chapter-baking.xhtml");
    assert!(baking.contains(&format!("images/{}.ill.jpg", f.loaf.id)));
}

#[test]
fn same_store_same_build_id() {
    let f = fixture();
    let (_a, a) = built(&f.store, now());
    let (_b, b) = built(&f.store, now());
    assert_eq!(a.build_id, b.build_id);
    assert_eq!(a.build_id.len(), 16);
    assert!(a.build_id.bytes().all(|c| c.is_ascii_hexdigit()));
    // The zip's timestamps don't count, and neither does a later build on the same day.
    std::thread::sleep(std::time::Duration::from_millis(2100));
    let (_c, c) = built(&f.store, now());
    assert_eq!(a.build_id, c.build_id);
}

#[test]
fn note_changes_build_id() {
    let f = fixture();
    let (_a, a) = built(&f.store, now());
    let mut s = f.chili.clone();
    s.note = Some("Add more cumin.".to_owned());
    f.store.update(&s).unwrap();
    let (_b, b) = built(&f.store, now());
    assert_ne!(a.build_id, b.build_id);
}

#[test]
fn build_leaves_the_dirty_flag_alone() {
    let f = fixture();
    assert!(f.store.is_dirty());
    let _ = built(&f.store, now());
    assert!(f.store.is_dirty(), "only delivery clears it");
}

#[test]
fn every_xhtml_well_formed() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    let mut seen = 0;
    for (name, bytes) in &e {
        if name.ends_with(".xhtml") || name.ends_with(".opf") || name.ends_with(".ncx") {
            let src = std::str::from_utf8(bytes).unwrap();
            xml(src);
            seen += 1;
        }
    }
    // contents, index, nav, 4 chapters, the OPF and the NCX.
    assert!(seen >= 9, "{seen}");
}

#[test]
fn manifest_hrefs_exist() {
    let f = fixture();
    let (e, _) = epub(&f.store, now());
    assert!(e.contains_key("mimetype"));
    assert!(e.contains_key("META-INF/container.xml"));
    let opf = text(&e, "OEBPS/content.opf");
    let doc = xml(opf);
    let mut count = 0;
    for item in doc.descendants().filter(|n| n.has_tag_name("item")) {
        let href = item.attribute("href").unwrap();
        assert!(e.contains_key(&format!("OEBPS/{href}")), "{href} missing");
        count += 1;
    }
    assert!(count > 10, "{count}");
    // The cover, the stylesheets and the pictures are all declared.
    assert!(opf.contains("properties=\"cover-image\""));
    assert!(opf.contains("href=\"cookbook.css\""));
    // Every spine idref names a manifest item.
    let ids: Vec<_> = doc
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .map(|n| n.attribute("id").unwrap())
        .collect();
    for r in doc.descendants().filter(|n| n.has_tag_name("itemref")) {
        assert!(ids.contains(&r.attribute("idref").unwrap()));
    }
    // The spine opens on the contents, and ends with the index.
    let s = spine(&e);
    assert_eq!(s[0], "contents.xhtml");
    assert!(s.contains(&"index.xhtml".to_owned()));
    // Every picture embedded is a JPEG that something uses.
    for name in e.keys().filter(|n| n.starts_with("OEBPS/images/")) {
        let file = name.trim_start_matches("OEBPS/");
        assert!(opf.contains(file), "{file} not in the manifest");
        assert_eq!(&e[name][..2], &[0xFF, 0xD8]);
    }
    // The cover is the generated 1600 x 2560 one.
    let cover = image::load_from_memory(&e["OEBPS/cover.jpg"]).unwrap();
    assert_eq!((cover.width(), cover.height()), (1600, 2560));
}

#[test]
fn an_empty_book_still_builds() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path().join("cookbook"));
    let (e, b) = epub(&store, now());
    assert_eq!(b.recipes, 0);
    xml(text(&e, "OEBPS/contents.xhtml"));
    assert!(!e.keys().any(|k| k.contains("chapter-")));
}
