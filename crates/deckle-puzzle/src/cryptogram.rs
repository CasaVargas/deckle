// SPDX-License-Identifier: GPL-3.0-or-later
//! Cryptograms (Puzzles tab design §2): a sentence enciphered letter for letter, the same way
//! throughout. The quotes are public domain, or the reader's own; a book of them is a
//! commonplace book you have to earn.

use rand::Rng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};

/// Characters a quote keeps besides letters and spaces.
const KEPT: &str = ",.;:!?'\"-";
/// A quote's length in letters, for a page to carry comfortably.
pub const LETTERS: std::ops::RangeInclusive<usize> = 24..=120;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cryptogram {
    /// Upper case, letters and a little punctuation, spaces between words.
    pub plain: String,
    /// `plain` with each letter replaced by its cipher letter.
    pub cipher: String,
    /// `key[p]` is the cipher letter for plain letter `p` (0 = A).
    pub key: [u8; 26],
    /// Cipher letters given to the solver as a start, most frequent first.
    pub given: Vec<char>,
    /// Who said it, printed with the answer.
    pub source: String,
}

impl Cryptogram {
    /// The plain letter a cipher letter stands for.
    pub fn decode(&self, cipher: char) -> Option<char> {
        let c = cipher.to_ascii_uppercase();
        self.key
            .iter()
            .position(|&k| (b'A' + k) as char == c)
            .map(|p| (b'A' + p as u8) as char)
    }

    /// The cipher's words, for a page: each a run of letters with its punctuation attached.
    pub fn words(&self) -> Vec<&str> {
        self.cipher.split(' ').filter(|w| !w.is_empty()).collect()
    }
}

/// `text` as a cryptogram prints it: upper case, letters, spaces and a little punctuation,
/// one space between words. `None` when fewer than [`LETTERS`] letters are left, or more.
pub fn clean(text: &str) -> Option<String> {
    let folded = deunicode::deunicode(text.trim());
    let kept: String = folded
        .chars()
        .map(|c| match c {
            c if c.is_ascii_alphabetic() => c.to_ascii_uppercase(),
            c if c.is_whitespace() => ' ',
            c if KEPT.contains(c) => c,
            _ => ' ',
        })
        .collect();
    let joined = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    let letters = joined.chars().filter(char::is_ascii_alphabetic).count();
    LETTERS.contains(&letters).then_some(joined)
}

/// A key with no letter standing for itself, from `rng`.
pub fn key(rng: &mut StdRng) -> [u8; 26] {
    loop {
        let mut k: Vec<u8> = (0..26).collect();
        k.shuffle(rng);
        if k.iter().enumerate().all(|(p, &c)| p as u8 != c) {
            return k.try_into().expect("26 letters");
        }
    }
}

/// `plain` (already [`clean`]) enciphered with `key`, with `reveals` of its most frequent
/// letters given.
pub fn make(plain: &str, source: &str, reveals: usize, rng: &mut StdRng) -> Cryptogram {
    let key = key(rng);
    let cipher: String = plain
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                (b'A' + key[(c as u8 - b'A') as usize]) as char
            } else {
                c
            }
        })
        .collect();
    let mut counts = [0usize; 26];
    for c in plain.chars().filter(char::is_ascii_alphabetic) {
        counts[(c as u8 - b'A') as usize] += 1;
    }
    let mut order: Vec<usize> = (0..26).filter(|&p| counts[p] > 0).collect();
    // frequent first; ties by a random order, so two books don't always give E
    let tie: Vec<u32> = (0..26).map(|_| rng.random()).collect();
    order.sort_by_key(|&p| (std::cmp::Reverse(counts[p]), tie[p]));
    let given = order
        .into_iter()
        .take(reveals)
        .map(|p| (b'A' + key[p]) as char)
        .collect();
    Cryptogram {
        plain: plain.to_owned(),
        cipher,
        key,
        given,
        source: source.to_owned(),
    }
}

/// Public-domain lines, for a book with no quotes of its own.
pub const QUOTES: &[(&str, &str)] = &[
    (
        "It is a truth universally acknowledged, that a single man in possession of a good fortune, must be in want of a wife.",
        "Jane Austen, Pride and Prejudice",
    ),
    (
        "It was the best of times, it was the worst of times.",
        "Charles Dickens, A Tale of Two Cities",
    ),
    (
        "I went to the woods because I wished to live deliberately.",
        "Henry David Thoreau, Walden",
    ),
    (
        "The man who does not read has no advantage over the man who cannot read.",
        "Mark Twain",
    ),
    (
        "Whatever our souls are made of, his and mine are the same.",
        "Emily Brontë, Wuthering Heights",
    ),
    (
        "All that we see or seem is but a dream within a dream.",
        "Edgar Allan Poe",
    ),
    ("Not all those who wander are lost.", "J. R. R. Tolkien"),
    (
        "It does not do to dwell on dreams and forget to live.",
        "J. K. Rowling",
    ),
    ("There is no friend as loyal as a book.", "Ernest Hemingway"),
    (
        "We are all in the gutter, but some of us are looking at the stars.",
        "Oscar Wilde, Lady Windermere's Fan",
    ),
    (
        "The only way to get rid of a temptation is to yield to it.",
        "Oscar Wilde, The Picture of Dorian Gray",
    ),
    (
        "A room without books is like a body without a soul.",
        "Cicero",
    ),
    (
        "Beware; for I am fearless, and therefore powerful.",
        "Mary Shelley, Frankenstein",
    ),
    (
        "Happy families are all alike; every unhappy family is unhappy in its own way.",
        "Leo Tolstoy, Anna Karenina",
    ),
    (
        "It is never too late to be what you might have been.",
        "George Eliot",
    ),
    (
        "The world is a book and those who do not travel read only one page.",
        "Augustine of Hippo",
    ),
    (
        "I am no bird; and no net ensnares me: I am a free human being with an independent will.",
        "Charlotte Brontë, Jane Eyre",
    ),
    (
        "Some books are to be tasted, others to be swallowed, and some few to be chewed and digested.",
        "Francis Bacon",
    ),
    (
        "There is nothing either good or bad, but thinking makes it so.",
        "William Shakespeare, Hamlet",
    ),
    (
        "All the world's a stage, and all the men and women merely players.",
        "William Shakespeare, As You Like It",
    ),
    (
        "The course of true love never did run smooth.",
        "William Shakespeare, A Midsummer Night's Dream",
    ),
    (
        "Last night I dreamt I went to Manderley again.",
        "Daphne du Maurier, Rebecca",
    ),
    (
        "Whenever you feel like criticizing anyone, just remember that all the people in this world haven't had the advantages that you've had.",
        "F. Scott Fitzgerald, The Great Gatsby",
    ),
    (
        "So we beat on, boats against the current, borne back ceaselessly into the past.",
        "F. Scott Fitzgerald, The Great Gatsby",
    ),
    (
        "The mass of men lead lives of quiet desperation.",
        "Henry David Thoreau, Walden",
    ),
    (
        "Nothing is so painful to the human mind as a great and sudden change.",
        "Mary Shelley, Frankenstein",
    ),
    (
        "In the middle of the journey of our life I found myself within a dark wood.",
        "Dante Alighieri, Inferno",
    ),
    (
        "It was a bright cold day in April, and the clocks were striking thirteen.",
        "George Orwell, 1984",
    ),
    (
        "Until I feared I would lose it, I never loved to read. One does not love breathing.",
        "Harper Lee, To Kill a Mockingbird",
    ),
    ("A thing of beauty is a joy for ever.", "John Keats"),
    (
        "Hope is the thing with feathers that perches in the soul.",
        "Emily Dickinson",
    ),
    (
        "Do I contradict myself? Very well then I contradict myself, I am large, I contain multitudes.",
        "Walt Whitman, Song of Myself",
    ),
    (
        "Two roads diverged in a wood, and I took the one less traveled by.",
        "Robert Frost",
    ),
    ("That is no country for old men.", "W. B. Yeats"),
    (
        "Water, water, every where, nor any drop to drink.",
        "Samuel Taylor Coleridge",
    ),
    (
        "The best laid schemes of mice and men go often awry.",
        "Robert Burns",
    ),
    ("No man is an island, entire of itself.", "John Donne"),
    (
        "Tomorrow is always fresh, with no mistakes in it yet.",
        "L. M. Montgomery, Anne of Green Gables",
    ),
    (
        "Isn't it nice to think that tomorrow is a new day with no mistakes in it yet?",
        "L. M. Montgomery, Anne of Green Gables",
    ),
    (
        "Alice laughed. There's no use trying, she said; one can't believe impossible things.",
        "Lewis Carroll, Through the Looking-Glass",
    ),
    (
        "Begin at the beginning, the King said, very gravely, and go on till you come to the end: then stop.",
        "Lewis Carroll, Alice's Adventures in Wonderland",
    ),
    (
        "The world is full of obvious things which nobody by any chance ever observes.",
        "Arthur Conan Doyle, The Hound of the Baskervilles",
    ),
    (
        "When you have eliminated the impossible, whatever remains, however improbable, must be the truth.",
        "Arthur Conan Doyle, The Sign of the Four",
    ),
    (
        "It is only with the heart that one can see rightly; what is essential is invisible to the eye.",
        "Antoine de Saint-Exupéry, The Little Prince",
    ),
    (
        "Time is the longest distance between two places.",
        "Tennessee Williams, The Glass Menagerie",
    ),
    (
        "The truth is rarely pure and never simple.",
        "Oscar Wilde, The Importance of Being Earnest",
    ),
    (
        "Man is born free, and everywhere he is in chains.",
        "Jean-Jacques Rousseau",
    ),
    ("Well done is better than well said.", "Benjamin Franklin"),
    (
        "Let us read, and let us dance; these two amusements will never do any harm to the world.",
        "Voltaire",
    ),
    (
        "Doubt thou the stars are fire; doubt that the sun doth move; doubt truth to be a liar; but never doubt I love.",
        "William Shakespeare, Hamlet",
    ),
    (
        "Let me be a free man, free to travel, free to stop, free to work.",
        "Chief Joseph",
    ),
    (
        "Those who dream by day are cognizant of many things which escape those who dream only by night.",
        "Edgar Allan Poe",
    ),
];

/// A `--quotes` file's lines: `quote — source` (or `quote | source`, or a tab), `#` lines
/// skipped, each through [`clean`]; the quotes that were too short or too long, for a warning.
pub fn quotes_from_file(text: &str) -> (Vec<(String, String)>, Vec<String>) {
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (quote, source) = ["\t", " — ", " -- ", " | "]
            .iter()
            .find_map(|sep| line.rsplit_once(sep))
            .map(|(q, s)| (q.trim(), s.trim()))
            .unwrap_or((line, ""));
        match clean(quote) {
            Some(q) => out.push((q, source.to_owned())),
            None => skipped.push(quote.chars().take(40).collect()),
        }
    }
    (out, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn clean_folds_and_bounds() {
        assert_eq!(
            clean("  “Café — it's   fine!”  Isn't it? ").as_deref(),
            None,
            "too short"
        );
        let q = clean("It was the best of times, it was the worst of times.").unwrap();
        assert_eq!(q, "IT WAS THE BEST OF TIMES, IT WAS THE WORST OF TIMES.");
        assert!(clean(&"word ".repeat(40)).is_none(), "too long");
    }

    #[test]
    fn the_key_never_maps_a_letter_to_itself_and_decodes() {
        let mut rng = StdRng::seed_from_u64(3);
        let c = make(
            "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG, TWICE.",
            "test",
            2,
            &mut rng,
        );
        assert!(c.key.iter().enumerate().all(|(p, &k)| p as u8 != k));
        let mut back = String::new();
        for ch in c.cipher.chars() {
            back.push(if ch.is_ascii_alphabetic() {
                c.decode(ch).unwrap()
            } else {
                ch
            });
        }
        assert_eq!(back, c.plain);
        assert_eq!(c.given.len(), 2);
        // the first given is the cipher of the most frequent plain letter (O or E here)
        let first = c.decode(c.given[0]).unwrap();
        assert!("OE".contains(first), "{first}");
        assert_eq!(c.words().len(), 10);
    }

    #[test]
    fn the_quotes_all_fit_a_page() {
        assert!(QUOTES.len() >= 40);
        for (q, s) in QUOTES {
            assert!(clean(q).is_some(), "{q}");
            assert!(!s.is_empty());
        }
    }

    #[test]
    fn a_quotes_file_takes_several_separators() {
        let (got, skipped) = quotes_from_file(
            "# mine\nThe only thing we have to fear is fear itself. — F. D. Roosevelt\nTo be, or not to be, that is the question.\tShakespeare\nshort | x\n",
        );
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].1, "F. D. Roosevelt");
        assert_eq!(got[1].0, "TO BE, OR NOT TO BE, THAT IS THE QUESTION.");
        assert_eq!(skipped, ["short"]);
    }
}
