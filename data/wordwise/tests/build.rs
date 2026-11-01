// SPDX-License-Identifier: GPL-3.0-or-later
//! `sample.jsonl` is 22 real kaikki.org entries (Wiktionary, CC BY-SA 4.0), trimmed to the
//! fields the builder reads.

use std::collections::{HashMap, HashSet};
use std::io::Write;

use deckle_wordwise::lexicon::Lexicon;
use deckle_wordwise_build::{
    build, crossword_words, difficulty, load_wordfreq, puzzle_words, short_def, zipf,
};

fn freq() -> HashMap<String, f64> {
    [
        ("take", 5.6),
        ("took", 4.9),
        ("off", 6.0),
        ("bear", 4.0),
        ("bore", 3.9),
        ("bucket", 3.9),
        ("kick", 4.7),
        ("the", 7.7),
        ("lugubrious", 1.8),
        ("perspicacious", 1.3),
        ("bath", 4.3),
        ("beth", 3.9),
        ("house", 5.7),
        ("o'er", 2.5),
        ("ran", 5.0),
    ]
    .into_iter()
    .map(|(w, z)| (w.to_owned(), z))
    .collect()
}

#[test]
fn difficulty_bands() {
    assert_eq!(difficulty(None), None);
    assert_eq!(difficulty(Some(4.25)), None);
    assert_eq!(difficulty(Some(4.2)), Some(5));
    assert_eq!(difficulty(Some(3.5)), Some(4));
    assert_eq!(difficulty(Some(3.0)), Some(3));
    assert_eq!(difficulty(Some(2.5)), Some(2));
    assert_eq!(difficulty(Some(1.8)), Some(1));
}

#[test]
fn phrase_zipf_is_dominated_by_its_rarest_word() {
    let z = zipf(&freq(), "kick the bucket").unwrap();
    assert!((3.7..3.9).contains(&z), "{z}");
    assert_eq!(zipf(&freq(), "kick the pail"), None);
}

#[test]
fn short_glosses() {
    assert_eq!(
        short_def("Gloomy, mournful or dismal, especially to an exaggerated degree."),
        "Gloomy"
    );
    assert_eq!(
        short_def("Of acute discernment; having keen insight; mentally perceptive."),
        "Of acute discernment"
    );
    assert_eq!(
        short_def("The act of conversing; also, an instance of it."),
        "The act of conversing"
    );
    assert_eq!(
        short_def("Any well-bred, well-mannered, kind-hearted man."),
        "Any well-bred, well-mannered, kind-hearted man"
    );
    assert_eq!(
        short_def("A large mammal (family Ursidae) with shaggy fur."),
        "A large mammal with shaggy fur"
    );
    assert_eq!(
        short_def("An open, horse-drawn carriage with two wheels."),
        "An open, horse-drawn carriage with two wheels"
    );
}

#[test]
fn builds_a_lexicon_from_kaikki() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("ww.sqlite");
    let jsonl = include_bytes!("sample.jsonl");
    let stats = build(&jsonl[..], &freq(), &out, "test").unwrap();
    assert_eq!(stats.entries, 22);
    let lex = Lexicon::open(&out).unwrap();
    assert!(lex.version.starts_with("test+"), "{}", lex.version);
    // the build fingerprint is deterministic
    let again = dir.path().join("again.sqlite");
    build(&jsonl[..], &freq(), &again, "test").unwrap();
    assert_eq!(Lexicon::open(&again).unwrap().version, lex.version);

    let lug = lex.get("lugubrious").unwrap();
    assert_eq!((lug.pos.as_str(), lug.difficulty), ("adjective", 1));
    assert_eq!(lex.get("perspicacious").unwrap().difficulty, 1);
    // phrase, and its inflections
    let kick = lex.get("kicked the bucket").unwrap();
    assert_eq!(
        (kick.lemma.as_str(), kick.difficulty),
        ("kick the bucket", 5)
    );
    assert_eq!(lex.max_words, 3);
    // the main "bear" verb (carry, many senses) beats the stock-market etymology
    let bore = lex.get("borne").unwrap();
    assert_eq!((bore.lemma.as_str(), bore.pos.as_str()), ("bear", "verb"));
    // a form maps to the richer entry: "bore" (past of bear) over the one-sense tidal bore
    assert_eq!(lex.get("bore").unwrap().lemma, "bear");
    // too common, proper nouns, form-of only, obsolete forms: absent
    for w in [
        "take",
        "took",
        "house",
        "ran",
        "bath's",
        "beare",
        "more lugubrious",
    ] {
        assert!(lex.get(w).is_none(), "{w}");
    }
    assert!(lex.senses().iter().all(|s| s.lemma != "Bath"));
}

#[test]
fn reads_wordfreq_cbpack() {
    use rmpv::Value;
    let header = Value::Map(vec![
        (Value::from("format"), Value::from("cB")),
        (Value::from("version"), Value::from(1)),
    ]);
    let mut arr = vec![header];
    for i in 0..=300 {
        arr.push(Value::Array(match i {
            200 => vec![Value::from("the")],
            300 => vec![Value::from("bucket"), Value::from("pail")],
            _ => vec![],
        }));
    }
    let mut raw = Vec::new();
    rmpv::encode::write_value(&mut raw, &Value::Array(arr)).unwrap();
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gz.write_all(&raw).unwrap();
    let f = load_wordfreq(&gz.finish().unwrap()[..]).unwrap();
    assert_eq!(f["the"], 7.0);
    assert_eq!(f["pail"], 6.0);
    assert_eq!(f.len(), 3);
}

#[test]
fn dated_spellings_are_hinted_with_the_modern_word() {
    let mut f = freq();
    f.insert("connexion".into(), 2.3);
    let e: serde_json::Value = serde_json::from_str(
        r#"{"word":"connexion","pos":"noun","lang_code":"en","senses":[
            {"tags":["UK"],"glosses":["Dated spelling of connection."]}]}"#,
    )
    .unwrap();
    let s = deckle_wordwise_build::sense_from_entry(&e, &f).unwrap();
    assert_eq!(s.short_def, "connection");
    assert_eq!(s.full_def, "Dated spelling of connection");
}

/// Kaikki-shaped entries for the puzzle list (trimmed; glosses from Wiktionary, CC BY-SA 4.0).
const PUZZLE_SAMPLE: &str = r#"{"word":"card","pos":"noun","lang_code":"en","forms":[{"form":"cards","tags":["plural"]}],"senses":[{"glosses":["A playing card."],"topics":["card-games"]},{"glosses":["A flat, rectangular piece of stiff paper, used for writing on."]}]}
{"word":"cards","pos":"noun","lang_code":"en","senses":[{"glosses":["plural of card"],"tags":["form-of","plural"],"form_of":[{"word":"card"}]},{"glosses":["Any game played with a deck of fifty-two."]}]}
{"word":"area","pos":"noun","lang_code":"en","forms":[{"form":"areas","tags":["plural"]}],"senses":[{"glosses":["Any particular extent of space or surface; a region."]}]}
{"word":"bucket","pos":"noun","lang_code":"en","senses":[{"glosses":["A container made of rigid material, often with a handle."]}]}
{"word":"Leith","pos":"noun","lang_code":"en","senses":[{"glosses":["A port district of Edinburgh."]}]}
{"word":"re-up","pos":"verb","lang_code":"en","senses":[{"glosses":["To renew."]}]}
{"word":"adze","pos":"noun","lang_code":"en","senses":[{"glosses":["A cutting tool with a curved blade."]}]}
{"word":"dosh","pos":"noun","lang_code":"en","senses":[{"glosses":["Money."],"tags":["slang","UK"]}]}
{"word":"heck","pos":"intj","lang_code":"en","senses":[{"glosses":["A mild expletive."]}]}
{"word":"darn","pos":"verb","lang_code":"en","senses":[{"glosses":["To mend a hole by weaving thread."]}]}
{"word":"rose","pos":"adj","lang_code":"en","senses":[{"glosses":["Of a rose-coloured hue."]},{"glosses":["Like roses in scent."]}]}
{"word":"warm","pos":"adj","lang_code":"en","senses":[{"glosses":["Having a fairly high temperature."]}]}
{"word":"warm","pos":"verb","lang_code":"en","senses":[{"glosses":["To heat up."]},{"glosses":["To become ardent."]},{"glosses":["To favour increasingly."]}]}
{"word":"water","pos":"noun","lang_code":"en","senses":[{"glosses":["An inorganic compound found at room temperature and pressure as a clear, colourless, tasteless liquid; its solid form is ice"]},{"glosses":["A clear liquid that falls as rain."]}]}
{"word":"pane","pos":"noun","lang_code":"en","senses":[{"glosses":["A sheet of flat glass or clear plastic that is fitted into the frame of a window in a building or a vehicle or a greenhouse."]}]}
{"word":"Goa","pos":"name","lang_code":"en","senses":[{"glosses":["A state of India."]}]}
{"word":"goa","pos":"noun","lang_code":"en","senses":[{"glosses":["A Tibetan gazelle."]}]}
{"word":"Bath","pos":"name","lang_code":"en","senses":[{"glosses":["A city in Somerset, England."]}]}
{"word":"bath","pos":"noun","lang_code":"en","senses":[{"glosses":["A tub for washing the body."]}]}
{"word":"bath","pos":"verb","lang_code":"en","senses":[{"glosses":["To wash in a tub."]}]}
{"word":"Beth","pos":"name","lang_code":"en","senses":[{"glosses":["A female given name."]}]}
{"word":"beth","pos":"noun","lang_code":"en","senses":[{"glosses":["The second letter of the Phoenician alphabet."]},{"glosses":["The second letter of the Aramaic alphabet."]},{"glosses":["The second letter of the Syriac alphabet."]},{"glosses":["The second letter of the Hebrew alphabet."]}]}
{"word":"roar","pos":"verb","lang_code":"en","senses":[{"glosses":["To make a loud, deep cry, especially from pain, anger, or other strong emotion; to bellow."]}]}
{"word":"hose","pos":"noun","lang_code":"en","senses":[{"glosses":["Specifically, a flexible tube conveying water."]}]}
{"word":"wood","pos":"noun","lang_code":"en","senses":[{"glosses":["Firewood."]},{"glosses":["The substance making up the trunk of a tree."]}]}
{"word":"leaf","pos":"noun","lang_code":"en","senses":[{"glosses":["The usually green and flat organ of a plant that photosynthesizes."]}]}
"#;

fn puzzle_freq() -> HashMap<String, f64> {
    [
        ("card", 4.6),
        ("cards", 4.4),
        ("area", 5.3),
        ("bucket", 3.9),
        ("leith", 3.2),
        ("re-up", 3.1),
        ("adze", 1.9),
        ("dosh", 3.1),
        ("heck", 3.9),
        ("darn", 3.4),
        ("rose", 4.5),
        ("leaf", 4.0),
        ("wood", 4.7),
        ("warm", 4.5),
        ("water", 5.2),
        ("pane", 3.3),
        ("goa", 3.4),
        ("bath", 4.3),
        ("beth", 3.9),
        ("roar", 3.6),
        ("hose", 3.5),
    ]
    .into_iter()
    .map(|(w, z)| (w.to_owned(), z))
    .collect()
}

fn puzzle_list(blocked: &[&str]) -> Vec<(String, String)> {
    let blocklist: HashSet<String> = blocked.iter().map(|w| (*w).to_owned()).collect();
    puzzle_words(PUZZLE_SAMPLE.as_bytes(), &puzzle_freq(), &blocklist)
}

#[test]
fn puzzle_words_keeps_common_short_words() {
    let list = puzzle_list(&[]);
    let clue = |w: &str| list.iter().find(|(x, _)| x == w).map(|(_, c)| c.as_str());
    // the everyday sense, not the card-games one
    assert_eq!(
        clue("card"),
        Some("A flat, rectangular piece of stiff paper, used for writing on")
    );
    assert_eq!(
        clue("area"),
        Some("Any particular extent of space or surface")
    );
    assert_eq!(
        clue("leaf"),
        Some("The usually green and flat organ of a plant that photosynthesizes")
    );
    // the first entry in Wiktionary's order, not the one with more senses
    assert_eq!(clue("warm"), Some("Having a fairly high temperature"));
    // a fragment ("its solid form is ice") is no clue: the next sense is
    assert_eq!(clue("water"), Some("A clear liquid that falls as rain"));
    // a name's spelling whose lowercase entries are only nouns ("Goa", "Beth") is out; a word
    // used another way too stays
    assert_eq!(clue("goa"), None);
    assert_eq!(clue("beth"), None);
    assert_eq!(clue("bath"), Some("A tub for washing the body"));
    // the whole first clause when it fits, not its first comma part; no "Specifically,"
    assert_eq!(
        clue("roar"),
        Some("To make a loud, deep cry, especially from pain, anger, or other strong emotion")
    );
    assert_eq!(clue("hose"), Some("A flexible tube conveying water"));
    // too long to print beside a 5×5 grid
    assert_eq!(clue("pane"), None);
    let words: Vec<&str> = list.iter().map(|(w, _)| w.as_str()).collect();
    let mut sorted = words.clone();
    sorted.sort_unstable();
    assert_eq!(words, sorted);
}

#[test]
fn puzzle_words_filters() {
    let list = puzzle_list(&["darn"]);
    for w in ["cards", "bucket", "leith", "re-up", "adze", "dosh", "darn"] {
        assert!(list.iter().all(|(x, _)| x != w), "{w} should be out");
    }
    // without the blocklist "darn" is in
    assert!(puzzle_list(&[]).iter().any(|(x, _)| x == "darn"));
}

#[test]
fn puzzle_clue_never_contains_answer() {
    let list = puzzle_list(&[]);
    assert!(list.iter().all(|(w, _)| w != "rose"));
    // inside another word counts too: "Firewood" gives "wood" away
    assert!(list.contains(&(
        "wood".into(),
        "The substance making up the trunk of a tree".into()
    )));
    for (w, c) in &list {
        assert!(!c.to_lowercase().contains(w.as_str()), "{w}: {c}");
    }
}

/// Crosswords take longer words than the 5×5 mini, each with its frequency.
#[test]
fn crossword_words_keeps_longer_words() {
    let blocklist = HashSet::new();
    let list = crossword_words(PUZZLE_SAMPLE.as_bytes(), &puzzle_freq(), &blocklist);
    let bucket = list.iter().find(|(w, _, _)| w == "bucket").expect("bucket");
    assert!((bucket.1 - 3.9).abs() < 1e-9);
    assert_eq!(
        bucket.2,
        "A container made of rigid material, often with a handle"
    );
    assert!(list.iter().any(|(w, _, _)| w == "card"));
    assert!(puzzle_list(&[]).iter().all(|(w, _)| w != "bucket"));
    for (w, _, c) in &list {
        assert!(!c.to_lowercase().contains(w.as_str()), "{w}: {c}");
    }
}

const INFLECTED: &str = r#"{"word":"card","pos":"noun","lang_code":"en","senses":[{"glosses":["A flat, rectangular piece of stiff paper, used for writing on."]}]}
{"word":"cards","pos":"noun","lang_code":"en","senses":[{"tags":["form-of","plural"],"form_of":[{"word":"card"}],"glosses":["plural of card"]}]}
{"word":"bake","pos":"verb","lang_code":"en","senses":[{"glosses":["To cook in an oven."]}]}
{"word":"baked","pos":"verb","lang_code":"en","senses":[{"tags":["form-of","participle","past"],"form_of":[{"word":"bake"}],"glosses":["simple past and past participle of bake"]}]}
{"word":"baking","pos":"verb","lang_code":"en","senses":[{"tags":["form-of","participle","present"],"form_of":[{"word":"bake"}],"glosses":["present participle of bake"]}]}
{"word":"oven","pos":"noun","lang_code":"en","senses":[{"glosses":["A chamber used for baking or heating."]}]}
{"word":"ovens","pos":"noun","lang_code":"en","senses":[{"tags":["form-of","plural"],"form_of":[{"word":"oven"}],"glosses":["plural of oven"]}]}
{"word":"write","pos":"verb","lang_code":"en","senses":[{"glosses":["To form letters, words or symbols on a surface."]}]}
{"word":"written","pos":"verb","lang_code":"en","senses":[{"tags":["form-of","participle","past"],"form_of":[{"word":"write"}],"glosses":["past participle of write"]}]}
{"word":"prime","pos":"adj","lang_code":"en","senses":[{"glosses":["First in importance."]}]}
{"word":"prime","pos":"verb","lang_code":"en","senses":[{"glosses":["To prepare for use."]}]}
{"word":"primed","pos":"verb","lang_code":"en","senses":[{"tags":["form-of","participle","past"],"form_of":[{"word":"prime"}],"glosses":["simple past and past participle of prime"]}]}
{"word":"principal","pos":"adj","lang_code":"en","senses":[{"glosses":["Primary."]}]}
{"word":"principals","pos":"noun","lang_code":"en","senses":[{"tags":["form-of","plural"],"form_of":[{"word":"principal"}],"glosses":["plural of principal"]}]}
"#;

/// A crossword needs its plurals and verb forms: each takes its word's clue, marked.
#[test]
fn crossword_words_take_inflections_with_marked_clues() {
    let freq: HashMap<String, f64> = [
        ("card", 4.6),
        ("cards", 4.4),
        ("bake", 3.6),
        ("baked", 3.5),
        ("baking", 3.6),
        ("oven", 3.9),
        ("ovens", 2.9),
        ("write", 5.0),
        ("written", 4.5),
        ("prime", 4.8),
        ("primed", 3.1),
        ("principal", 4.5),
        ("principals", 3.5),
    ]
    .into_iter()
    .map(|(w, z)| (w.to_owned(), z))
    .collect();
    let list = crossword_words(INFLECTED.as_bytes(), &freq, &HashSet::new());
    let clue = |w: &str| {
        list.iter()
            .find(|(x, _, _)| x == w)
            .map(|(_, _, c)| c.as_str())
    };
    assert_eq!(
        clue("cards"),
        Some("Flat, rectangular piece of stiff paper, used for writing on (plural)")
    );
    assert_eq!(clue("baked"), Some("To cook in an oven (past tense)"));
    assert_eq!(clue("baking"), Some("To cook in an oven (-ing form)"));
    // the clue of the word's sense the form belongs to: "primed" is the verb's, not the
    // adjective's; "principals" has no noun to take one from
    assert_eq!(clue("primed"), Some("To prepare for use (past tense)"));
    // only a simple past is a past tense: "written" isn't
    assert_eq!(
        clue("written"),
        Some("To form letters, words or symbols on a surface (past participle)")
    );
    assert_eq!(clue("principals"), None);
    // a clue that gives the answer away is no clue: "baking or heating" holds "ovens"? no, but
    // the plural's clue must not hold the plural itself
    assert!(
        list.iter()
            .all(|(w, _, c)| !c.to_lowercase().contains(w.as_str()))
    );
    let words: Vec<&str> = list.iter().map(|(w, _, _)| w.as_str()).collect();
    let mut sorted = words.clone();
    sorted.sort_unstable();
    assert_eq!(words, sorted);
    // the 5×5 mini's list stays without them
    let mini = puzzle_words(INFLECTED.as_bytes(), &freq, &HashSet::new());
    assert!(mini.iter().all(|(w, _)| w != "cards" && w != "baked"));
}
