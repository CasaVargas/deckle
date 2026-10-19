// SPDX-License-Identifier: GPL-3.0-or-later
use deckle_kfx::text::Run;
use deckle_xray::asc::EntityKind;
use deckle_xray::extract::{Candidate, heuristic};

fn runs(paras: &[&str]) -> Vec<Run> {
    let mut pid = 0;
    paras
        .iter()
        .map(|p| {
            let r = Run {
                pid,
                text: (*p).to_owned(),
            };
            pid += p.chars().count() as i64 + 1;
            r
        })
        .collect()
}

fn find<'a>(c: &'a [Candidate], label: &str) -> Option<&'a Candidate> {
    c.iter().find(|x| x.label == label)
}

#[test]
fn finds_recurring_names_and_places_with_positions() {
    let text = runs(&[
        "Chapter One",
        "It is a truth universally acknowledged, that a single man in possession of a good fortune, must be in want of a wife.",
        "Mr. Bennet was among the earliest of those who waited on Mr. Bingley. He had always intended to visit him.",
        "Elizabeth Bennet looked at Jane. Jane smiled at Elizabeth, and Mr. Darcy watched them both from Netherfield.",
        "Darcy said nothing. Elizabeth laughed. Netherfield was quiet that evening, and Longbourn quieter still.",
        "The next day Elizabeth walked to Longbourn with Mr. Darcy and Jane.",
    ]);
    let c = heuristic(&text, "en");
    // "Elizabeth" is the common form; the single "Elizabeth Bennet" mention is too rare to absorb it
    let eliz = find(&c, "Elizabeth").expect("Elizabeth");
    assert_eq!(
        eliz.kind,
        EntityKind::Person,
        "speech verb nearby makes her a person"
    );
    assert!(eliz.occurrences.len() >= 3, "{:?}", eliz.occurrences);
    assert_eq!(eliz.occurrences[0], (text[3].pid, 9));
    assert!(find(&c, "Elizabeth Bennet").is_none(), "one mention only");
    let darcy = find(&c, "Mr. Darcy").expect("Mr. Darcy");
    assert_eq!(darcy.kind, EntityKind::Person);
    assert_eq!(
        darcy.quote, "Jane smiled at Elizabeth, and Mr. Darcy watched them both from Netherfield.",
        "the dot in an honorific does not end the sentence"
    );
    let bennet = heuristic(
        &runs(&["Mr. Bennet was among the earliest. Mr. Bennet smiled."]),
        "en",
    );
    assert_eq!(
        find(&bennet, "Mr. Bennet").unwrap().quote,
        "Mr. Bennet was among the earliest.",
        "a quote starting with an honorific keeps its whole sentence"
    );
    let curly = heuristic(
        &runs(&["‘Mr. Darcy’ laughed aloud. ‘Mr. Darcy’ sighed again."]),
        "en",
    );
    assert_eq!(
        find(&curly, "Mr. Darcy").unwrap().quote,
        "‘Mr. Darcy’ laughed aloud.",
        "a multi-byte quote mark before the honorific"
    );
    assert!(darcy.occurrences.len() >= 3);
    assert_eq!(find(&c, "Netherfield").unwrap().kind, EntityKind::Term);
    assert_eq!(find(&c, "Longbourn").unwrap().kind, EntityKind::Term);
    assert!(find(&c, "Jane").is_some());
    // sentence-initial words and chapter headings are not entities
    for bad in [
        "The",
        "It",
        "Chapter One",
        "Chapter",
        "He",
        "Darcy said nothing",
    ] {
        assert!(find(&c, bad).is_none(), "{bad} must not be an entity");
    }
    // quotes are spoiler-free first mentions
    assert!(
        eliz.quote.contains("Elizabeth Bennet looked at Jane"),
        "{}",
        eliz.quote
    );
}

#[test]
fn single_mentions_and_stopwords_are_dropped() {
    let text = runs(&[
        "Paris is lovely. Paris is big. Rome once.",
        "The Thames flows. The Thames is wide.",
    ]);
    let c = heuristic(&text, "en");
    assert!(find(&c, "Paris").is_some());
    assert!(find(&c, "Rome").is_none(), "one mention");
    assert!(find(&c, "Thames").is_some());
    assert!(find(&c, "The Thames").is_none());
}

#[test]
fn capitalized_common_words_drop_caps_and_calendar_words_are_not_entities() {
    let text = runs(&[
        "Allow me to say so. Allow me again. Allow it, please; I shall allow nothing else, nor allow that.",
        "HEN the day came, Thursday it was, in June. HEN again. On Thursday in June they left.",
        "Dawson brought the tea, and Dawson took it away; Dawson was tired.",
    ]);
    let c = heuristic(&text, "en");
    assert!(
        find(&c, "Allow").is_none(),
        "a capitalized verb that also appears lowercase"
    );
    assert!(find(&c, "HEN").is_none(), "drop-cap remnant");
    assert!(find(&c, "Thursday").is_none());
    assert!(find(&c, "June").is_none());
    assert!(
        find(&c, "Dawson").is_some(),
        "a real name never appears lowercase"
    );
}

/// Quality smoke on a real novel: set DECKLE_SMOKE_KFX to a monolithic KFX path.
#[test]
fn smoke_on_a_real_book_prints_the_top_entities() {
    let Ok(path) = std::env::var("DECKLE_SMOKE_KFX") else {
        return;
    };
    let kfx = std::fs::read(path).unwrap();
    let (runs, _) = deckle_kfx::text::content_runs(&kfx).unwrap();
    let c = heuristic(&runs, "en");
    for e in c.iter().take(25) {
        eprintln!(
            "{:>4}  {:?}  {}  aliases={:?}",
            e.occurrences.len(),
            e.kind,
            e.label,
            e.aliases
        );
    }
    assert!(c.len() > 10);
}
