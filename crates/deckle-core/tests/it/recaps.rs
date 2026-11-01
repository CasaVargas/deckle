// SPDX-License-Identifier: GPL-3.0-or-later
//! Chapter recaps (docs/superpowers/specs/2026-11-01-ai-extras-design.md).

use std::io::{Read, Write};

use deckle_core::recaps::{RecapOptions, RecapSummary, add_with, parts, word_count};
use deckle_core::xray::LlmConfig;
use deckle_xray::llm::Provider;

/// `n` distinct words, `w0 w1 …`.
fn words(n: usize, from: usize) -> String {
    (from..from + n)
        .map(|i| format!("w{i}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn word_count_counts_spaced_words() {
    assert_eq!(word_count("The cat sat.\n\nOn a mat"), 6);
}

#[test]
fn word_count_counts_each_cjk_char() {
    assert_eq!(word_count(&"猫".repeat(1000)), 1000);
    assert_eq!(word_count("彼は ねこ"), 4);
}

#[test]
fn parts_packs_paragraphs_in_order() {
    let text = (0..5)
        .map(|p| words(600, p * 600))
        .collect::<Vec<_>>()
        .join("\n\n");
    let got = parts(&text, 2_000);
    let counts: Vec<usize> = got.iter().map(|p| word_count(p)).collect();
    assert_eq!(counts, [1800, 1200]);
    let back: Vec<&str> = got.iter().flat_map(|p| p.split_whitespace()).collect();
    let want: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(back, want);
}

#[test]
fn parts_cuts_a_long_paragraph() {
    let got = parts(&words(5_000, 0), 2_000);
    let counts: Vec<usize> = got.iter().map(|p| word_count(p)).collect();
    assert_eq!(counts, [2000, 2000, 1000]);
}

#[test]
fn one_part_when_it_fits() {
    assert_eq!(parts(&words(900, 0), 40_000).len(), 1);
}

// ---------------------------------------------------------------------------------------------
// Recaps into the EPUB

/// An EPUB 3 with a nav; chapter `i` is `OEBPS/text/c{i}.xhtml`, its body `<h1>{title}</h1>` then
/// the given raw XHTML. `meta` goes into the OPF metadata; spine items in `nonlinear` get
/// `linear="no"`.
fn epub_with(chapters: &[(&str, &str)], meta: &str, nonlinear: &[usize]) -> Vec<u8> {
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let mut put = |name: &str, body: &str| {
        z.start_file(name, stored).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    };
    put("mimetype", "application/epub+zip");
    put(
        "META-INF/container.xml",
        "<?xml version=\"1.0\"?><container version=\"1.0\" \
         xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile \
         full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>\
         </rootfiles></container>",
    );
    let items: String = (0..chapters.len())
        .map(|i| {
            format!(
                "<item id=\"c{i}\" href=\"text/c{i}.xhtml\" media-type=\"application/xhtml+xml\"/>"
            )
        })
        .collect();
    let refs: String = (0..chapters.len())
        .map(|i| {
            if nonlinear.contains(&i) {
                format!("<itemref idref=\"c{i}\" linear=\"no\"/>")
            } else {
                format!("<itemref idref=\"c{i}\"/>")
            }
        })
        .collect();
    put(
        "OEBPS/content.opf",
        &format!(
            "<?xml version=\"1.0\"?><package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\">\
             <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>Pride</dc:title>\
             <dc:creator>Jane Austen</dc:creator><dc:language>en</dc:language>{meta}</metadata>\
             <manifest><item id=\"nav\" href=\"nav.xhtml\" properties=\"nav\" \
             media-type=\"application/xhtml+xml\"/>{items}</manifest><spine>{refs}</spine></package>"
        ),
    );
    let lis: String = chapters
        .iter()
        .enumerate()
        .map(|(i, (title, _))| format!("<li><a href=\"text/c{i}.xhtml\">{title}</a></li>"))
        .collect();
    put(
        "OEBPS/nav.xhtml",
        &format!(
            "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\" \
             xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>Contents</title></head>\
             <body><nav epub:type=\"toc\"><ol>{lis}</ol></nav></body></html>"
        ),
    );
    for (i, (title, body)) in chapters.iter().enumerate() {
        put(
            &format!("OEBPS/text/c{i}.xhtml"),
            &format!(
                "<?xml version=\"1.0\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head>\
                 <title>{title}</title></head><body><h1>{title}</h1>{body}</body></html>"
            ),
        );
    }
    z.finish().unwrap().into_inner()
}

fn epub(chapters: &[(&str, &str)]) -> Vec<u8> {
    epub_with(chapters, "", &[])
}

/// A paragraph of `n` words.
fn para(n: usize) -> String {
    format!("<p>{}</p>", words(n, 0))
}

fn read(bytes: &[u8], name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

fn chapter(bytes: &[u8], i: usize) -> String {
    read(bytes, &format!("OEBPS/text/c{i}.xhtml"))
}

fn local() -> LlmConfig {
    LlmConfig {
        provider: Provider::OpenAi,
        model: "test-model".into(),
        endpoint: "http://localhost:11434/v1".into(),
        api_key: String::new(),
    }
}

fn hosted() -> LlmConfig {
    LlmConfig {
        endpoint: String::new(),
        api_key: "k".into(),
        ..local()
    }
}

/// An OpenAI chat completion answering `text`.
fn completion(text: &str) -> String {
    serde_json::json!({"choices": [{"message": {"content": text}, "finish_reason": "stop"}]})
        .to_string()
}

struct Ran {
    bytes: Vec<u8>,
    summary: Option<RecapSummary>,
    /// Request bodies, in order.
    bodies: Vec<String>,
}

/// Runs [`add_with`]; `answer(n, body)` answers the `n`th request (from 0).
fn run(
    book: &[u8],
    llm: LlmConfig,
    cache_dir: Option<std::path::PathBuf>,
    mut answer: impl FnMut(usize, &str) -> Result<String, String>,
) -> Ran {
    let mut bodies = Vec::new();
    let mut post = |_url: &str, _h: &[(String, String)], body: &str| {
        let n = bodies.len();
        bodies.push(body.to_owned());
        answer(n, body).map(|t| completion(&t))
    };
    let (bytes, summary) = add_with(book, &RecapOptions { llm, cache_dir }, &mut post).unwrap();
    Ran {
        bytes,
        summary,
        bodies,
    }
}

fn parses(xhtml: &str) {
    roxmltree::Document::parse(xhtml).unwrap_or_else(|e| panic!("{e}: {xhtml}"));
}

const LABEL: &str = "AI-generated summary (test-model), not part of the original book.";

#[test]
fn long_chapters_end_with_a_recap_and_short_ones_dont() {
    let book = epub(&[
        ("One", &para(900)),
        ("Two", &para(300)),
        ("Three", &para(1200)),
    ]);
    let r = run(&book, local(), None, |n, _| {
        Ok(format!("Recap number {n}."))
    });
    let c0 = chapter(&r.bytes, 0);
    let c1 = chapter(&r.bytes, 1);
    let c2 = chapter(&r.bytes, 2);
    assert_eq!(c0.matches("class=\"deckle-recap\"").count(), 1, "{c0}");
    assert!(
        c0.contains("</p><aside class=\"deckle-recap\" id=\"deckle-recap-d0-s1\""),
        "{c0}"
    );
    assert!(c0.contains("Recap number 0.") && c0.contains(LABEL), "{c0}");
    assert!(c0.ends_with("</aside></body></html>"), "{c0}");
    assert!(!c1.contains("deckle-recap"), "{c1}");
    assert_eq!(c2.matches("class=\"deckle-recap\"").count(), 1);
    assert!(c2.contains("Recap number 1."), "{c2}");
    for x in [&c0, &c1, &c2] {
        parses(x);
    }
    for f in ["OEBPS/content.opf", "OEBPS/nav.xhtml"] {
        assert_eq!(read(&r.bytes, f), read(&book, f));
    }
    let s = r.summary.unwrap();
    assert_eq!((s.chapters, s.recapped, s.cached), (2, 2, 0));
    assert_eq!(s.model, "test-model");
    assert!(s.warnings.is_empty(), "{:?}", s.warnings);
}

#[test]
fn two_chapters_in_one_file_each_get_their_own() {
    let body = format!(
        "<h2>Two</h2>{}<div class=\"chapter\">\n<h2>Three</h2>{}</div>",
        para(900),
        para(900)
    );
    let r = run(&epub(&[("One", &body)]), local(), None, |n, _| {
        Ok(format!("Recap number {n}."))
    });
    let c0 = chapter(&r.bytes, 0);
    assert_eq!(c0.matches("class=\"deckle-recap\"").count(), 2, "{c0}");
    assert!(
        c0.contains("</p><aside class=\"deckle-recap\" id=\"deckle-recap-d0-s2\""),
        "{c0}"
    );
    let first = c0.find("id=\"deckle-recap-d0-s2\"").unwrap();
    let div = c0.find("<div class=\"chapter\">").unwrap();
    assert!(
        first < div,
        "the first recap must come before the wrapper: {c0}"
    );
    // The last one ends the file, after the wrapper closes.
    assert!(
        c0.contains("</p></div><aside class=\"deckle-recap\" id=\"deckle-recap-d0-s3\""),
        "{c0}"
    );
    parses(&c0);
    assert_eq!(r.bodies.len(), 2);
    assert!(r.bodies[0].contains("Chapter: Two"), "{}", r.bodies[0]);
    assert!(r.bodies[1].contains("Chapter: Three"), "{}", r.bodies[1]);
}

#[test]
fn request_is_spoiler_safe() {
    let one = format!("<p>ALPHA</p>{}", para(900));
    let two = format!("<p>OMEGA</p>{}", para(900));
    let r = run(
        &epub(&[("One", &one), ("Two", &two)]),
        local(),
        None,
        |_, _| Ok("Recap.".into()),
    );
    let first = &r.bodies[0];
    assert!(
        first.contains("Do not guess at or hint about anything that happens later."),
        "{first}"
    );
    assert!(first.contains("ALPHA") && !first.contains("OMEGA"));
    let v: serde_json::Value = serde_json::from_str(first).unwrap();
    assert_eq!(v["max_tokens"], 4096);
}

#[test]
fn hosted_openai_uses_max_completion_tokens() {
    let r = run(&epub(&[("One", &para(900))]), hosted(), None, |_, _| {
        Ok("Recap.".into())
    });
    let v: serde_json::Value = serde_json::from_str(&r.bodies[0]).unwrap();
    assert_eq!(v["max_completion_tokens"], 4096);
}

#[test]
fn a_long_chapter_on_a_local_server_is_split_then_merged() {
    let body: String = (0..5).map(|_| para(1000)).collect();
    let book = epub(&[("One", &body)]);
    let r = run(&book, local(), None, |n, _| {
        Ok(if n < 3 {
            format!("Part answer {n}.")
        } else {
            "Merged.".into()
        })
    });
    assert_eq!(r.bodies.len(), 4);
    let merge = &r.bodies[3];
    let at = |s: &str| {
        merge
            .find(s)
            .unwrap_or_else(|| panic!("{s} missing: {merge}"))
    };
    assert!(
        at("Part answer 0.") < at("Part answer 1.") && at("Part answer 1.") < at("Part answer 2.")
    );
    assert!(chapter(&r.bytes, 0).contains("Merged."));

    let r = run(&book, hosted(), None, |_, _| Ok("Recap.".into()));
    assert_eq!(r.bodies.len(), 1);
}

#[test]
fn answer_is_escaped_and_cleaned() {
    let r = run(&epub(&[("One", &para(900))]), local(), None, |_, _| {
        Ok("Recap: \"Tom & <Jerry>\"".into())
    });
    let c0 = chapter(&r.bytes, 0);
    assert!(c0.contains(">Tom &amp; &lt;Jerry&gt;</p>"), "{c0}");
    parses(&c0);
}

#[test]
fn one_failure_is_a_warning() {
    let book = epub(&[
        ("One", &para(900)),
        ("Two", &para(900)),
        ("Three", &para(900)),
    ]);
    let r = run(&book, local(), None, |n, _| {
        if n == 1 {
            Err("status: 500".into())
        } else {
            Ok("Recap.".into())
        }
    });
    assert!(chapter(&r.bytes, 0).contains("deckle-recap"));
    assert!(!chapter(&r.bytes, 1).contains("deckle-recap"));
    assert!(chapter(&r.bytes, 2).contains("deckle-recap"));
    let s = r.summary.unwrap();
    assert_eq!(s.recapped, 2);
    assert_eq!(
        s.warnings,
        ["Chapter recap skipped for Two: LLM request failed: status: 500"]
    );
}

#[test]
fn all_failures_leave_the_book_unchanged() {
    let names = ["A", "B", "C", "D", "E"];
    let long = para(900);
    let chapters: Vec<(&str, &str)> = names.iter().map(|n| (*n, long.as_str())).collect();
    let book = epub(&chapters);
    // Empty answers: failures that aren't the server going away, so every chapter is tried.
    let r = run(&book, local(), None, |_, _| Ok(String::new()));
    assert_eq!(r.bodies.len(), 5);
    assert_eq!(r.bytes, book);
    let s = r.summary.unwrap();
    assert_eq!(s.recapped, 0);
    assert_eq!(s.warnings.len(), 4, "{:?}", s.warnings);
    assert_eq!(s.warnings[3], "…and 2 more chapter recaps skipped");
}

#[test]
fn cache_makes_a_second_run_free() {
    let tmp = tempfile::tempdir().unwrap();
    let book = epub(&[("One", &para(900)), ("Two", &para(900))]);
    let first = run(&book, local(), Some(tmp.path().to_path_buf()), |n, _| {
        Ok(format!("Recap number {n}."))
    });
    assert_eq!(first.bodies.len(), 2);
    let second = run(&book, local(), Some(tmp.path().to_path_buf()), |_, _| {
        Err("no request expected".into())
    });
    assert!(second.bodies.is_empty());
    for i in 0..2 {
        assert_eq!(chapter(&second.bytes, i), chapter(&first.bytes, i));
    }
    let s = second.summary.unwrap();
    assert_eq!((s.recapped, s.cached), (2, 2));
}

#[test]
fn already_recapped_and_fixed_layout_are_unchanged() {
    let book = epub(&[("One", &para(900))]);
    let once = run(&book, local(), None, |_, _| Ok("Recap.".into()));
    let twice = run(&once.bytes, local(), None, |_, _| Ok("Again.".into()));
    assert!(twice.bodies.is_empty());
    assert!(twice.summary.is_none());
    assert_eq!(twice.bytes, once.bytes);

    let fixed = epub_with(
        &[("One", &para(900))],
        "<meta property=\"rendition:layout\">pre-paginated</meta>",
        &[],
    );
    let r = run(&fixed, local(), None, |_, _| Ok("Recap.".into()));
    assert!(r.bodies.is_empty() && r.summary.is_none());
    assert_eq!(r.bytes, fixed);
}

#[test]
fn nonlinear_spine_items_are_skipped() {
    let book = epub_with(&[("One", &para(900)), ("Notes", &para(900))], "", &[1]);
    let r = run(&book, local(), None, |_, _| Ok("Recap.".into()));
    assert_eq!(r.bodies.len(), 1);
    assert!(!chapter(&r.bytes, 1).contains("deckle-recap"));
}

// ---------------------------------------------------------------------------------------------
// Review fixes

#[test]
fn non_latin_answers_are_kept() {
    for answer in [
        "猫が庭で眠り、少年はそれを見つめる。",
        "В главе Анна уезжает из города.",
        "이 장에서 소년은 떠난다.",
    ] {
        let r = run(&epub(&[("One", &para(900))]), local(), None, |_, _| {
            Ok(answer.to_owned())
        });
        assert!(chapter(&r.bytes, 0).contains(answer), "{answer}");
    }
}

#[test]
fn characters_xml_forbids_are_dropped() {
    let r = run(&epub(&[("One", &para(900))]), local(), None, |_, _| {
        Ok("Tom\u{1}leaves\u{FFFF} town.".into())
    });
    let c0 = chapter(&r.bytes, 0);
    assert!(c0.contains(">Tomleaves town.</p>"), "{c0}");
    parses(&c0);
}

#[test]
fn a_dead_server_stops_after_three_failures() {
    let names = ["A", "B", "C", "D", "E", "F"];
    let long = para(900);
    let chapters: Vec<(&str, &str)> = names.iter().map(|n| (*n, long.as_str())).collect();
    let r = run(&epub(&chapters), local(), None, |_, _| {
        Err("timed out".into())
    });
    assert_eq!(
        r.bodies.len(),
        3,
        "stops asking after three failures in a row"
    );
    let s = r.summary.unwrap();
    assert_eq!(s.recapped, 0);
    assert_eq!(
        s.warnings.last().unwrap(),
        "The LLM stopped answering: 3 more chapters skipped"
    );
}

#[test]
fn a_toc_anchor_before_a_heading_stays_with_its_chapter() {
    let body = format!(
        "<h2>Two</h2>{}<a id=\"ch3\"></a><h2>Three</h2>{}",
        para(900),
        para(900)
    );
    let r = run(&epub(&[("One", &body)]), local(), None, |_, _| {
        Ok("Recap.".into())
    });
    let c0 = chapter(&r.bytes, 0);
    assert!(
        c0.contains("</aside><a id=\"ch3\"></a><h2>Three</h2>"),
        "{c0}"
    );
}
