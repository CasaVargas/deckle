// SPDX-License-Identifier: GPL-3.0-or-later
//! Deckle's comic EPUB as an image-only PDF for a reMarkable (fixed-layout spec §5, §12.2):
//! one page per image, in spine order, each fitted to the tablet's screen and centred on a
//! page the screen's size. reMarkable's EPUB reader reflows, so this is the only fixed page
//! there.

use std::io::{Cursor, Write};

use image::{ColorType, DynamicImage, ImageFormat};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, TextStr};

use crate::pages::{Screen, fit};

/// What [`from_epub`] made.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Made {
    pub pdf: Vec<u8>,
    /// Pages written.
    pub pages: usize,
    /// Page images that wouldn't decode, left out.
    pub skipped: usize,
}

/// The EPUB in `epub` (Deckle's own, from [`crate::convert`]) as a PDF: its spine's pages,
/// each image passed through [`fit`] for `screen`, on pages `page` points wide and high (the
/// screen at its dpi). An image that won't decode is left out and counted.
pub fn from_epub(epub: &[u8], screen: Screen, page: (f32, f32)) -> Result<Made, String> {
    use deckle_limits::{MAX_COMIC_BYTES, MAX_COMIC_PAGE_BYTES};
    let mut zip = zip::ZipArchive::new(Cursor::new(epub)).map_err(|e| e.to_string())?;
    crate::check_declared_sizes(&zip, MAX_COMIC_PAGE_BYTES, MAX_COMIC_BYTES)
        .map_err(|e| e.to_string())?;
    let mut read = |name: &str| -> Result<Vec<u8>, String> {
        let f = zip
            .by_name(name)
            .map_err(|_| format!("{name}: not in the EPUB"))?;
        deckle_limits::read_capped(f, MAX_COMIC_PAGE_BYTES).map_err(|e| format!("{name}: {e}"))
    };
    let container = String::from_utf8_lossy(&read("META-INF/container.xml")?).into_owned();
    let container = match deckle_limits::parse_xml(&container, Default::default()) {
        // Refused for its depth or entities, it says so rather than "no OPF named".
        Err(e @ (deckle_limits::XmlError::TooDeep | deckle_limits::XmlError::EntityMarkup)) => {
            return Err(format!("container.xml: {e}"));
        }
        parsed => parsed,
    };
    let opf_path = container
        .ok()
        .and_then(|d| {
            d.descendants()
                .find(|n| n.has_tag_name("rootfile"))?
                .attribute("full-path")
                .map(str::to_owned)
        })
        .ok_or("no OPF named in container.xml")?;
    let opf_dir = parent(&opf_path);
    let opf = String::from_utf8_lossy(&read(&opf_path)?).into_owned();
    let (book, pages) = spine(&opf, &opf_dir)?;
    if pages.is_empty() {
        return Err("the comic has no pages".into());
    }

    let mut images = Vec::with_capacity(pages.len());
    let mut skipped = 0;
    for xhtml_path in &pages {
        let xhtml = String::from_utf8_lossy(&read(xhtml_path)?).into_owned();
        let Some(src) = img_src(&xhtml) else {
            skipped += 1;
            continue;
        };
        let bytes = read(&join(&parent(xhtml_path), &src))?;
        match page_image(&bytes, screen) {
            Ok(img) => images.push(img),
            Err(_) => skipped += 1,
        }
    }
    if images.is_empty() {
        return Err("none of the comic's pages would decode".into());
    }
    let pdf = write(&book, &images, screen, page);
    Ok(Made {
        pdf,
        pages: images.len(),
        skipped,
    })
}

/// Title and author from the OPF, for the PDF's Info.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Book {
    title: String,
    author: String,
}

/// The OPF's title and author, and its spine's XHTML paths (in the zip's naming) in order.
fn spine(opf: &str, opf_dir: &str) -> Result<(Book, Vec<String>), String> {
    let doc = deckle_limits::parse_xml(opf, Default::default())
        .map_err(|e| format!("the OPF didn't parse: {e}"))?;
    let text_of = |tag: &str| -> String {
        doc.descendants()
            .find(|n| n.has_tag_name(tag))
            .and_then(|n| n.text())
            .map(|t| t.trim().to_owned())
            .unwrap_or_default()
    };
    let book = Book {
        title: text_of("title"),
        author: text_of("creator"),
    };
    let href_of = |id: &str| -> Option<String> {
        doc.descendants()
            .find(|n| n.has_tag_name("item") && n.attribute("id") == Some(id))?
            .attribute("href")
            .map(|h| join(opf_dir, h))
    };
    let pages = doc
        .descendants()
        .filter(|n| n.has_tag_name("itemref"))
        .filter_map(|r| r.attribute("idref").and_then(href_of))
        .collect();
    Ok((book, pages))
}

/// The one `<img src>` of a page's XHTML.
fn img_src(xhtml: &str) -> Option<String> {
    let doc = deckle_limits::parse_xml(xhtml, Default::default()).ok()?;
    doc.descendants()
        .find(|n| n.has_tag_name("img"))?
        .attribute("src")
        .map(str::to_owned)
}

fn parent(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_owned(),
        None => String::new(),
    }
}

/// `rel` against `dir`, with `..` and `.` folded, as the zip names it.
fn join(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for p in rel.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

/// One page's image as the PDF carries it.
enum PageImage {
    /// A JPEG as it is: DCTDecode, grey or RGB.
    Jpeg {
        bytes: Vec<u8>,
        w: u32,
        h: u32,
        gray: bool,
    },
    /// Raw 8-bit samples, deflated: lossless for line art.
    Raw {
        data: Vec<u8>,
        w: u32,
        h: u32,
        gray: bool,
    },
}

impl PageImage {
    fn size(&self) -> (u32, u32) {
        match self {
            PageImage::Jpeg { w, h, .. } | PageImage::Raw { w, h, .. } => (*w, *h),
        }
    }
}

/// `bytes` fitted to `screen`, then as the PDF takes it.
fn page_image(bytes: &[u8], screen: Screen) -> Result<PageImage, String> {
    let fitted = fit(bytes, screen)?;
    let bytes = fitted.as_deref().unwrap_or(bytes);
    let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
    let img = image::load_from_memory_with_format(bytes, format).map_err(|e| e.to_string())?;
    let (w, h) = (img.width(), img.height());
    // A baseline JPEG in grey or RGB goes in whole; anything else (CMYK, 16-bit) is decoded.
    if format == ImageFormat::Jpeg && matches!(img.color(), ColorType::L8 | ColorType::Rgb8) {
        return Ok(PageImage::Jpeg {
            bytes: bytes.to_vec(),
            w,
            h,
            gray: img.color() == ColorType::L8,
        });
    }
    let gray = !screen.color || matches!(img.color(), ColorType::L8 | ColorType::La8);
    let pixels = if gray {
        DynamicImage::ImageLuma8(img.to_luma8()).into_bytes()
    } else {
        DynamicImage::ImageRgb8(img.to_rgb8()).into_bytes()
    };
    Ok(PageImage::Raw {
        data: deflate(&pixels),
        w,
        h,
        gray,
    })
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).expect("in memory");
    e.finish().expect("in memory")
}

/// The PDF: a page per image, the image centred at the screen's pixel-to-point scale.
fn write(book: &Book, images: &[PageImage], screen: Screen, page: (f32, f32)) -> Vec<u8> {
    let (pw, ph) = page;
    // One screen pixel in points; the same on both axes for a screen stated at one dpi.
    let scale = pw / screen.width as f32;
    let mut pdf = Pdf::new();
    let mut next = 1;
    let mut alloc = || {
        let r = Ref::new(next);
        next += 1;
        r
    };
    let catalog = alloc();
    let tree = alloc();
    let page_refs: Vec<Ref> = images.iter().map(|_| alloc()).collect();
    let content_refs: Vec<Ref> = images.iter().map(|_| alloc()).collect();
    let image_refs: Vec<Ref> = images.iter().map(|_| alloc()).collect();
    let info = alloc();

    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree)
        .kids(page_refs.iter().copied())
        .count(page_refs.len() as i32);

    for (i, img) in images.iter().enumerate() {
        let (w, h) = img.size();
        let (dw, dh) = (w as f32 * scale, h as f32 * scale);
        let (x, y) = ((pw - dw) / 2.0, (ph - dh) / 2.0);
        let name = format!("Im{i}");
        let mut p = pdf.page(page_refs[i]);
        p.parent(tree)
            .media_box(Rect::new(0.0, 0.0, pw, ph))
            .contents(content_refs[i]);
        p.resources()
            .x_objects()
            .pair(Name(name.as_bytes()), image_refs[i]);
        p.finish();

        let mut c = Content::new();
        c.save_state()
            .transform([dw, 0.0, 0.0, dh, x, y])
            .x_object(Name(name.as_bytes()))
            .restore_state();
        pdf.stream(content_refs[i], &c.finish());

        match img {
            PageImage::Jpeg { bytes, w, h, gray } => {
                let mut x = pdf.image_xobject(image_refs[i], bytes);
                x.width(*w as i32).height(*h as i32).bits_per_component(8);
                x.filter(Filter::DctDecode);
                if *gray {
                    x.color_space().device_gray();
                } else {
                    x.color_space().device_rgb();
                }
            }
            PageImage::Raw { data, w, h, gray } => {
                let mut x = pdf.image_xobject(image_refs[i], data);
                x.width(*w as i32).height(*h as i32).bits_per_component(8);
                x.filter(Filter::FlateDecode);
                if *gray {
                    x.color_space().device_gray();
                } else {
                    x.color_space().device_rgb();
                }
            }
        }
    }

    let mut d = pdf.document_info(info);
    if !book.title.is_empty() {
        d.title(TextStr(&book.title));
    }
    if !book.author.is_empty() {
        d.author(TextStr(&book.author));
    }
    d.producer(TextStr("Deckle"));
    d.finish();
    pdf.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::path::Path;

    fn image(w: u32, h: u32, format: ImageFormat) -> Vec<u8> {
        let img = image::RgbImage::from_fn(w, h, |x, _| {
            if x < w / 2 {
                image::Rgb([200, 30, 30])
            } else {
                image::Rgb([30, 30, 200])
            }
        });
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, format).unwrap();
        out.into_inner()
    }

    /// A comic EPUB made from `files` as a CBZ.
    fn comic(dir: &Path, files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        let cbz = dir.join("c.cbz");
        let mut zw = zip::ZipWriter::new(std::fs::File::create(&cbz).unwrap());
        for (n, bytes) in files {
            zw.start_file(*n, zip::write::SimpleFileOptions::default())
                .unwrap();
            zw.write_all(bytes).unwrap();
        }
        zw.finish().unwrap();
        let epub = dir.join("c.epub");
        crate::convert(&cbz, &epub).unwrap();
        std::fs::read(epub).unwrap()
    }

    /// A quarter-size reMarkable 2, grey, and its page in points at 226 dpi.
    const RM2: Screen = Screen {
        width: 351,
        height: 468,
        color: false,
    };
    const PAGE: (f32, f32) = (351.0 / 226.0 * 72.0, 468.0 / 226.0 * 72.0);

    /// How often `needle` occurs in the PDF, read with whitespace folded to single spaces
    /// (pdf-writer lays a dictionary's keys out one a line).
    fn count(pdf: &[u8], needle: &str) -> usize {
        let text: String = String::from_utf8_lossy(pdf)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        text.matches(needle).count()
    }

    #[test]
    fn each_page_is_an_image_on_a_page_the_screens_size() {
        let tmp = tempfile::tempdir().unwrap();
        let epub = comic(
            tmp.path(),
            &[
                (
                    "ComicInfo.xml",
                    b"<ComicInfo><Title>Ink</Title><Writer>Ada</Writer></ComicInfo>".to_vec(),
                ),
                ("1.jpg", image(300, 400, ImageFormat::Jpeg)),
                ("2.png", image(300, 400, ImageFormat::Png)),
                // Twice the screen: fitted down.
                ("3.jpg", image(702, 936, ImageFormat::Jpeg)),
            ],
        );
        let made = from_epub(&epub, RM2, PAGE).unwrap();
        assert_eq!((made.pages, made.skipped), (3, 0));
        let pdf = made.pdf;
        assert!(pdf.starts_with(b"%PDF-"));
        assert_eq!(count(&pdf, "/Type /Page "), 3);
        assert_eq!(count(&pdf, "/Subtype /Image"), 3);
        // The JPEGs pass through, grey (the screen is); the PNG is deflated raw samples.
        assert_eq!(count(&pdf, "/Filter /DCTDecode"), 2);
        assert_eq!(count(&pdf, "/Filter /FlateDecode"), 1);
        assert_eq!(count(&pdf, "/ColorSpace /DeviceGray"), 3);
        // Every page is the screen in points.
        let (pw, ph) = PAGE;
        let media = format!("/MediaBox [0 0 {pw} {ph}]");
        assert_eq!(count(&pdf, &media), 3, "{media}");
        // The big page was fitted to the screen.
        assert_eq!(count(&pdf, "/Width 351 /Height 468"), 1);
        assert_eq!(count(&pdf, "/Width 300 /Height 400"), 2);
        let s = String::from_utf8_lossy(&pdf);
        assert!(
            s.contains("/Title (Ink)") && s.contains("/Author (Ada)"),
            "info"
        );
    }

    #[test]
    fn colour_stays_on_a_colour_screen_and_a_broken_page_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let epub = comic(
            tmp.path(),
            &[
                ("1.jpg", image(300, 400, ImageFormat::Jpeg)),
                ("2.png", image(300, 400, ImageFormat::Png)),
            ],
        );
        // Break the PNG inside the EPUB: its header stays, so the importer took it.
        let mut zin = zip::ZipArchive::new(Cursor::new(&epub[..])).unwrap();
        let mut zout = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for i in 0..zin.len() {
            let mut f = zin.by_index(i).unwrap();
            let name = f.name().unwrap().into_owned();
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes).unwrap();
            if name.ends_with("p0002.png") {
                bytes.truncate(40);
            }
            zout.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zout.write_all(&bytes).unwrap();
        }
        let broken = zout.finish().unwrap().into_inner();
        let colour = Screen { color: true, ..RM2 };
        let made = from_epub(&broken, colour, PAGE).unwrap();
        assert_eq!((made.pages, made.skipped), (1, 1));
        assert_eq!(count(&made.pdf, "/ColorSpace /DeviceRGB"), 1);
    }

    #[test]
    fn pages_follow_the_spine_so_a_manga_reads_right_to_left() {
        let tmp = tempfile::tempdir().unwrap();
        // A cover, then a spread: the spread's right half must come before its left.
        let epub = comic(
            tmp.path(),
            &[
                (
                    "ComicInfo.xml",
                    b"<ComicInfo><Manga>YesAndRightToLeft</Manga></ComicInfo>".to_vec(),
                ),
                ("1.jpg", image(300, 400, ImageFormat::Jpeg)),
                ("2.jpg", image(600, 400, ImageFormat::Jpeg)),
            ],
        );
        let made = from_epub(&epub, RM2, PAGE).unwrap();
        assert_eq!(made.pages, 3);
        // Three images in page order: the cover, then the halves (300 wide each).
        assert_eq!(count(&made.pdf, "/Width 300 /Height 400"), 3);
        // Right half first: red is on the left of the whole, so the first half after the
        // cover is the blue one. Decode the second image's JPEG and look at a pixel.
        let jpegs: Vec<usize> = made
            .pdf
            .windows(2)
            .enumerate()
            .filter(|(_, w)| w == &[0xFF, 0xD8])
            .map(|(i, _)| i)
            .collect();
        assert_eq!(jpegs.len(), 3);
        let second = &made.pdf[jpegs[1]..];
        let img = image::load_from_memory_with_format(second, ImageFormat::Jpeg)
            .unwrap()
            .to_luma8();
        // Grey of pure blue (30,30,200) is darker than grey of red (200,30,30).
        let blue_grey = 0.299 * 30.0 + 0.587 * 30.0 + 0.114 * 200.0;
        let got = img.get_pixel(150, 200)[0] as f32;
        assert!(
            (got - blue_grey).abs() < 12.0,
            "second page is the right half: {got}"
        );
    }

    #[test]
    fn a_book_without_pages_is_refused() {
        let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (n, b) in [
            ("mimetype", "application/epub+zip"),
            (
                "META-INF/container.xml",
                "<container><rootfiles><rootfile full-path=\"OEBPS/content.opf\"/></rootfiles></container>",
            ),
            (
                "OEBPS/content.opf",
                "<package><metadata/><manifest/><spine/></package>",
            ),
        ] {
            zw.start_file(n, zip::write::SimpleFileOptions::default())
                .unwrap();
            zw.write_all(b.as_bytes()).unwrap();
        }
        let epub = zw.finish().unwrap().into_inner();
        assert_eq!(
            from_epub(&epub, RM2, PAGE).unwrap_err(),
            "the comic has no pages"
        );
    }

    /// A container.xml nested too deep says so, not "no OPF named in container.xml".
    #[test]
    fn a_container_nested_too_deep_says_so() {
        let deep = format!(
            "<container>{}{}</container>",
            "<x>".repeat(300),
            "</x>".repeat(300)
        );
        let mut zw = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zw.start_file(
            "META-INF/container.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zw.write_all(deep.as_bytes()).unwrap();
        let epub = zw.finish().unwrap().into_inner();
        assert_eq!(
            from_epub(&epub, RM2, PAGE).unwrap_err(),
            "container.xml: this file is nested more than 256 levels deep"
        );
    }

    #[test]
    fn paths_join_as_the_zip_names_them() {
        assert_eq!(
            join("OEBPS/pages", "../images/p0001.jpg"),
            "OEBPS/images/p0001.jpg"
        );
        assert_eq!(join("", "OEBPS/content.opf"), "OEBPS/content.opf");
        assert_eq!(join("OEBPS", "./nav.xhtml"), "OEBPS/nav.xhtml");
    }
}
