// SPDX-License-Identifier: GPL-3.0-or-later
//! An [`Edition`] → the EPUB's files: front page, one XHTML per section, nav, NCX, OPF.

use crate::{Edition, Gathered, esc, long_date};

/// `(path inside the EPUB, bytes)`. `mimetype` and `META-INF/container.xml` included.
pub type Files = Vec<(String, Vec<u8>)>;

pub(crate) const STYLE: &str = r#"body { font-family: serif; margin: 0 4%; }
h1, h2, h3 { font-family: serif; }
.masthead { text-align: center; border-bottom: 3px double black; margin-bottom: 0.6em; }
.ears { font-size: 0.75em; text-transform: uppercase; letter-spacing: 0.1em; margin: 0; }
.paper { font-size: 2.4em; margin: 0.2em 0 0.1em 0; }
.dateline { font-style: italic; margin: 0 0 0.4em 0; }
.weather { text-align: center; font-weight: bold; border-bottom: 1px solid black; padding-bottom: 0.4em; }
.section-head { text-align: center; border-top: 3px double black; border-bottom: 1px solid black;
  text-transform: uppercase; letter-spacing: 0.12em; font-size: 1.4em; padding: 0.2em 0; margin-bottom: 0.2em; }
.kicker { text-align: center; font-style: italic; font-size: 0.85em; margin-top: 0; }
.unavailable { text-align: center; font-style: italic; }
h2.headline { font-size: 1.2em; margin: 1.2em 0 0.1em 0; }
.meta { font-size: 0.8em; font-style: italic; margin: 0 0 0.5em 0; }
.rule { border-top: 1px solid black; margin: 1em 25%; }
p { text-indent: 0; margin: 0 0 0.5em 0; }
.link { font-size: 0.8em; }
.big { font-size: 2.2em; font-weight: bold; margin: 0.1em 0; }
.lo { font-size: 0.6em; font-weight: normal; }
table.strip { width: 100%; border-collapse: collapse; border-top: 1px solid black; border-bottom: 1px solid black; }
table.strip td { width: 20%; text-align: center; vertical-align: top; padding: 0.3em 0.1em; font-size: 0.85em; }
table.strip img { width: 2.2em; height: 2.2em; }
.wday { font-weight: bold; letter-spacing: 0.08em; }
.word { font-style: italic; }
table.day { width: 100%; border-collapse: collapse; }
table.day td { border-bottom: 1px solid black; padding: 0.2em 0.2em; vertical-align: top; }
td.when { white-space: nowrap; font-weight: bold; width: 5.5em; }
.rain { font-weight: bold; margin-top: 0.4em; }
.credit { font-size: 0.7em; text-transform: uppercase; letter-spacing: 0.08em; text-align: center; margin-top: 1.2em; }
td.num { text-align: right; }
td.day { font-weight: bold; }
.balls { font-size: 1.3em; font-weight: bold; }
.ball { border: 1px solid black; padding: 0 0.25em; }
.special { background-color: black; color: white; }
.label { font-size: 0.75em; font-weight: normal; text-transform: uppercase; }
.event .year { font-weight: bold; }
.small { font-size: 0.85em; }
.figure { text-align: center; margin: 0.6em 0 0.8em 0; }
.figure img, p img { width: 100%; }
.caption { display: block; font-size: 0.8em; font-style: italic; text-align: left; margin-top: 0.2em; }
h3.subhead { font-size: 1.05em; margin: 1em 0 0.3em 0; }
blockquote { margin: 0.6em 1.2em; font-style: italic; }
.source { font-size: 0.8em; font-style: italic; margin-top: 0.8em; }
.note { font-style: italic; }
table.cols { width: 100%; border-collapse: collapse; margin: 0.6em 0; }
table.cols td { width: 50%; vertical-align: top; padding: 0.4em 0.5em; border-top: 1px solid black; }
table.cols .label { font-variant: small-caps; text-transform: none; letter-spacing: 0.06em; margin: 0 0 0.1em 0; }
h2.lead { font-size: 1.6em; margin: 0.6em 0 0.1em 0; }
h2.lead a { text-decoration: none; color: black; }
ul.index { list-style-type: none; padding-left: 0; }
ul.index li { border-bottom: 1px solid black; padding: 0.3em 0; }
table.grid { border-collapse: collapse; margin: 0.6em auto; }
table.grid td { width: 2em; height: 2em; border: 1px solid black; padding: 0; vertical-align: top;
  text-align: center; font-weight: bold; }
table.grid td.b { background-color: black; }
table.grid span.n { display: block; font-size: 0.5em; font-weight: normal; text-align: left;
  line-height: 1; padding: 0.1em 0 0 0.15em; }
table.grid.small td { width: 1.1em; height: 1.1em; font-size: 0.85em; vertical-align: middle; }
p.clue { margin: 0 0 0.25em 0; }
p.ladder { text-align: center; font-weight: bold; letter-spacing: 0.05em; }
p.score { font-weight: bold; }
"#;

pub(crate) fn page(title: &str, body: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\" xml:lang=\"en\" lang=\"en\">\n\
         <head><meta charset=\"utf-8\"/><title>{}</title>\
         <link rel=\"stylesheet\" type=\"text/css\" href=\"style.css\"/></head>\n<body>\n{body}\n</body>\n</html>\n",
        esc(title)
    )
    .into_bytes()
}

fn section_file(i: usize) -> String {
    format!("s{}.xhtml", i + 1)
}

/// Photo `n` of article `j` in section `i`, relative to the OPF.
fn photo_file(i: usize, j: usize, n: usize) -> String {
    format!("images/s{}a{}p{}.jpg", i + 1, j + 1, n + 1)
}

fn front_page(g: &Gathered) -> Vec<u8> {
    let e = &g.edition;
    let mut ears = format!("No. {}", e.number());
    if !e.place.is_empty() {
        ears.push_str(&format!(" · {}", e.place));
    }
    let mut body = format!(
        "<div class=\"masthead\"><p class=\"ears\">{}</p><h1 class=\"paper\">{}</h1>\
         <p class=\"dateline\">{}</p></div>\n",
        esc(&ears),
        esc(&e.paper),
        esc(&long_date(e.date))
    );
    if let Some(w) = &e.weather_line {
        body.push_str(&format!("<p class=\"weather\">{}</p>\n", esc(w)));
    }
    let plan = crate::front::front_plan(g);
    let meta_of = |s: usize, a: usize| {
        let art = &e.sections[s].articles[a];
        let meta = match &art.meta {
            Some(m) => format!("{} · {m}", e.sections[s].title),
            None => e.sections[s].title.clone(),
        };
        format!("<span class=\"meta\">{}</span>", esc(&meta))
    };
    let link = |s: usize, a: usize| {
        format!(
            "<a href=\"{}#a{}\">{}</a>",
            section_file(s),
            a + 1,
            esc(&e.sections[s].articles[a].title)
        )
    };
    if let Some((s, a)) = plan.lead {
        body.push_str(&format!(
            "<h2 class=\"lead\" id=\"t{s}-{a}\">{}</h2>\n<p>{}</p>\n",
            link(s, a),
            meta_of(s, a)
        ));
        if let Some(t) = &e.sections[s].articles[a].teaser {
            body.push_str(&format!("<p>{}</p>\n", esc(t)));
        }
    }
    if !plan.tiles.is_empty() {
        body.push_str("<table class=\"cols\">\n");
        for row in plan.tiles.chunks(2) {
            body.push_str("<tr>");
            for &(s, a) in row {
                body.push_str(&format!(
                    "<td><p class=\"label\">{}</p><a id=\"t{s}-{a}\" href=\"{}#a{}\">{}</a><br/>{}</td>",
                    esc(&e.sections[s].title),
                    section_file(s),
                    a + 1,
                    esc(&e.sections[s].articles[a].title),
                    meta_of(s, a)
                ));
            }
            if row.len() == 1 {
                body.push_str("<td></td>");
            }
            body.push_str("</tr>\n");
        }
        body.push_str("</table>\n");
    }
    body.push_str("<h2>Inside today</h2>\n<ul class=\"index\">\n");
    for (i, s) in e.sections.iter().enumerate() {
        let count = match (s.articles.len(), &s.unavailable) {
            (0, Some(_)) => "unavailable".to_owned(),
            (1, _) => "1 item".to_owned(),
            (n, _) => format!("{n} items"),
        };
        body.push_str(&format!(
            "<li><a href=\"{}\">{}</a> <span class=\"meta\">{count}</span></li>\n",
            section_file(i),
            esc(&s.title)
        ));
    }
    body.push_str("</ul>\n");
    page(&e.full_title(), &body)
}

fn section_page(e: &Edition, i: usize) -> Vec<u8> {
    let s = &e.sections[i];
    let mut body = format!("<h1 class=\"section-head\">{}</h1>\n", esc(&s.title));
    if let Some(k) = &s.kicker {
        body.push_str(&format!("<p class=\"kicker\">{}</p>\n", esc(k)));
    }
    for (j, a) in s.articles.iter().enumerate() {
        if j > 0 {
            body.push_str("<div class=\"rule\"></div>\n");
        }
        body.push_str(&format!(
            "<h2 class=\"headline\" id=\"a{}\">{}</h2>\n",
            j + 1,
            esc(&a.title)
        ));
        if let Some(m) = &a.meta {
            body.push_str(&format!("<p class=\"meta\">{}</p>\n", esc(m)));
        }
        let mut story = a.body.clone();
        for n in 0..a.photos.len() {
            story = story.replace(&crate::sources::feeds::photo_token(n), &photo_file(i, j, n));
        }
        body.push_str(&story);
        body.push('\n');
    }
    if let Some(u) = &s.unavailable {
        body.push_str(&format!("<p class=\"unavailable\">{}</p>\n", esc(u)));
    }
    page(&s.title, &body)
}

fn nav(e: &Edition) -> Vec<u8> {
    let mut ol = String::from("<li><a href=\"front.xhtml\">Front page</a></li>\n");
    for (i, s) in e.sections.iter().enumerate() {
        ol.push_str(&format!(
            "<li><a href=\"{}\">{}</a>",
            section_file(i),
            esc(&s.title)
        ));
        if !s.articles.is_empty() {
            ol.push_str("<ol>");
            for (j, a) in s.articles.iter().enumerate() {
                ol.push_str(&format!(
                    "<li><a href=\"{}#a{}\">{}</a></li>",
                    section_file(i),
                    j + 1,
                    esc(&a.title)
                ));
            }
            ol.push_str("</ol>");
        }
        ol.push_str("</li>\n");
    }
    page(
        "Contents",
        &format!("<nav epub:type=\"toc\" id=\"toc\"><h1>Contents</h1><ol>\n{ol}</ol></nav>"),
    )
}

fn ncx(e: &Edition, uid: &str) -> Vec<u8> {
    let mut order = 0;
    let mut next = || {
        order += 1;
        order
    };
    let point = |id: String, n: u32, label: &str, src: &str, inner: &str| {
        format!(
            "<navPoint id=\"{id}\" playOrder=\"{n}\"><navLabel><text>{}</text></navLabel>\
             <content src=\"{src}\"/>{inner}</navPoint>\n",
            esc(label)
        )
    };
    let mut map = point("front".into(), next(), "Front page", "front.xhtml", "");
    for (i, s) in e.sections.iter().enumerate() {
        let n = next();
        let mut inner = String::new();
        for (j, a) in s.articles.iter().enumerate() {
            inner.push_str(&point(
                format!("s{}a{}", i + 1, j + 1),
                next(),
                &a.title,
                &format!("{}#a{}", section_file(i), j + 1),
                "",
            ));
        }
        map.push_str(&point(
            format!("s{}", i + 1),
            n,
            &s.title,
            &section_file(i),
            &inner,
        ));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n\
         <head><meta name=\"dtb:uid\" content=\"{}\"/><meta name=\"dtb:depth\" content=\"2\"/></head>\n\
         <docTitle><text>{}</text></docTitle>\n<navMap>\n{map}</navMap>\n</ncx>\n",
        esc(uid),
        esc(&e.full_title())
    )
    .into_bytes()
}

fn opf(e: &Edition, uid: &str) -> Vec<u8> {
    let mut manifest = String::from(
        "<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n\
         <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n\
         <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n\
         <item id=\"cover-image\" href=\"cover.jpg\" media-type=\"image/jpeg\" properties=\"cover-image\"/>\n\
         <item id=\"front\" href=\"front.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
    );
    let mut spine = String::from("<itemref idref=\"front\"/>\n");
    for (i, s) in e.sections.iter().enumerate() {
        for (j, a) in s.articles.iter().enumerate() {
            for n in 0..a.photos.len() {
                manifest.push_str(&format!(
                    "<item id=\"p{}-{}-{}\" href=\"{}\" media-type=\"image/jpeg\"/>\n",
                    i + 1,
                    j + 1,
                    n + 1,
                    photo_file(i, j, n)
                ));
            }
        }
    }
    for (n, (href, _)) in e.assets.iter().enumerate() {
        manifest.push_str(&format!(
            "<item id=\"x{}\" href=\"{}\" media-type=\"image/png\"/>\n",
            n + 1,
            esc(href)
        ));
    }
    for i in 0..e.sections.len() {
        manifest.push_str(&format!(
            "<item id=\"s{n}\" href=\"{}\" media-type=\"application/xhtml+xml\"/>\n",
            section_file(i),
            n = i + 1
        ));
        spine.push_str(&format!("<itemref idref=\"s{}\"/>\n", i + 1));
    }
    spine.push_str("<itemref idref=\"nav\" linear=\"no\"/>\n");
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"uid\" xml:lang=\"en\">\n\
         <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n\
         <dc:identifier id=\"uid\">{uid}</dc:identifier>\n\
         <dc:title>{title}</dc:title>\n\
         <dc:creator id=\"creator\">{author}</dc:creator>\n\
         <meta refines=\"#creator\" property=\"file-as\">{author}</meta>\n\
         <dc:language>en</dc:language>\n\
         <dc:publisher>{author}</dc:publisher>\n\
         <dc:date>{date}</dc:date>\n\
         <meta property=\"dcterms:modified\">{date}T00:00:00Z</meta>\n\
         <meta name=\"cover\" content=\"cover-image\"/>\n\
         </metadata>\n<manifest>\n{manifest}</manifest>\n<spine toc=\"ncx\">\n{spine}</spine>\n</package>\n",
        uid = esc(uid),
        title = esc(&e.full_title()),
        author = crate::AUTHOR,
        date = e.date,
    )
    .into_bytes()
}

/// Every file of the edition's EPUB. `cover` is the JPEG from [`crate::cover::render`].
pub fn render(g: &Gathered, cover: Vec<u8>) -> Files {
    let e = &g.edition;
    let uid = format!("urn:deckle:digest:{}", e.date);
    let mut files: Files = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".into(),
            b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
              <container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">\
              <rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
              </rootfiles></container>\n"
                .to_vec(),
        ),
        ("OEBPS/content.opf".into(), opf(e, &uid)),
        ("OEBPS/toc.ncx".into(), ncx(e, &uid)),
        ("OEBPS/nav.xhtml".into(), nav(e)),
        ("OEBPS/style.css".into(), STYLE.as_bytes().to_vec()),
        ("OEBPS/cover.jpg".into(), cover),
        ("OEBPS/front.xhtml".into(), front_page(g)),
    ];
    for (href, png) in &e.assets {
        files.push((format!("OEBPS/{href}"), png.clone()));
    }
    for i in 0..e.sections.len() {
        files.push((format!("OEBPS/{}", section_file(i)), section_page(e, i)));
        for (j, a) in e.sections[i].articles.iter().enumerate() {
            for (n, jpeg) in a.photos.iter().enumerate() {
                files.push((format!("OEBPS/{}", photo_file(i, j, n)), jpeg.clone()));
            }
        }
    }
    files
}
