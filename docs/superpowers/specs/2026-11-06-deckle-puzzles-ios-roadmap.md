# Deckle Puzzles for iPhone and iPad: roadmap

Deckle Puzzles is an iOS and iPadOS app that makes crosswords, word searches and the other
puzzles Deckle already sets for the paper, and lets you solve them the way the device is best at:
with Apple Pencil on an iPad, with a friend over FaceTime, from a widget, or by asking Siri. Apple
Intelligence writes themed word lists and clues; Deckle's own generators build every grid.

This document is the roadmap: the decisions it rests on, then each feature as a phase with what
it is, how it's built, what it needs and when it's done. Sizes are S (days), M (a week or two)
and L (several weeks), for one person who knows the code.

## 1. Decisions

**Native app, shared engine.** The app is SwiftUI. The desktop app's Tauri web view can't reach
PencilKit, Scribble, SharePlay, Live Activities, widgets or App Intents, and most of this roadmap
is those. The puzzles themselves come from the Rust generators the paper and the books already
use, in the `deckle-puzzle` crate, called from Swift through UniFFI. One engine means a puzzle
made on the iPad is the same puzzle the Mac would make from the same seed.

**The model never builds a grid.** A language model miscounts letters and breaks crossings. The
model's part is the one `deckle_digest::puzzle::ai` already has: a subject in, a list of words
and clues out, every word checked against the word list and the blocklist, every clue checked
for its own answer, and a failure leaves the puzzle as it was. The generators place the words.

**Three tiers of model, one contract.**

| Tier | When | Notes |
|---|---|---|
| On-device (Foundation Models) | Default, offline, free | Theme words, plain and kid clues, titles. Guided generation (`@Generable`) gives typed output, so there's no JSON to repair. |
| Private Cloud Compute | Trivia and cryptic clues, image input | Free only while the developer is in the App Store Small Business Program with under 2 million first-time downloads across all their apps. Daily limits Apple doesn't publish. Six months to move off it after leaving those terms. Every PCC call has an on-device fallback. |
| A server model (Claude, Gemini packages) | Optional, later | Same Swift API; billed to the provider account. Only if PCC limits bite. |

**Everything works without Apple Intelligence.** Apple Intelligence needs an iPhone 15 Pro or
later, an M-series iPad, and a supported region and language. Without it the app uses the
bundled word lists and clues (`words.tsv.gz`, `words-long.tsv.gz`), as the paper does with
the AI setting off. Only the features marked *AI* in this roadmap need it.

**Licensing comes first.** Deckle is GPL-3.0, and the App Store's terms of use conflict with the
GPL. Before any App Store build, the copyright holder decides: dual-license `deckle-puzzle` (and
anything else the app links) for the app, which needs every contributor to those files to agree,
or rewrite what can't be relicensed. Nothing below ships until this is settled.

## 2. Phases at a glance

| Phase | What | Size | Needs AI |
|---|---|---|---|
| 0 | Foundations: engine crate, iOS build, Swift bindings, licensing | M | No |
| 1 | The app: make and solve, by touch | L | No |
| 2 | Apple Pencil | M–L | No |
| 3 | Apple Intelligence: themes and clues | M | AI |
| 4 | Siri, Shortcuts and the system | M | Partly |
| 5 | The Lock Screen and Home Screen: Live Activity and widgets | M | No |
| 6 | Together: SharePlay and Game Center | M–L | No |
| 7 | The camera: scan, photograph, check | L | Partly |
| 8 | Delight: haptics, read aloud, bilingual, cover art | S each | Some |
| 9 | Everywhere: iCloud, Handoff, Mac, Watch | M–L | No |
| 10 | Experiments: AR word search | L | No |

Phases 4 to 9 don't depend on each other; each needs phase 1, and the Pencil, camera and
AI-backed parts need phases 2 and 3 where noted.

## Phase 0: Foundations (M)

**The engine on its own.** `crates/deckle-puzzle` holds the generators: word lists, crossword
fill, word search, ladder, cryptogram, sudoku, nonogram and maze. It has no network, file or OS
access, and builds for `aarch64-apple-ios` and `aarch64-apple-ios-sim`; CI checks both. The
paper, the books and the AI clue writer stay in `deckle-digest` and reach the generators through
`deckle_digest::puzzle`, so no path there changed. *Done in the first change.*

**Swift bindings.** A thin `deckle-puzzle-ffi` crate with UniFFI: `make_crossword(seed, words,
size)`, `make_search(seed, words, size)`, and so on, each returning plain records (cells,
numbers, clues, word positions). Built as an XCFramework (device, simulator) by a script in
`tools/`. The FFI crate owns the API Swift sees, so the engine's own types can keep changing.

**Determinism across platforms.** A test that the same seed and words give the same grid on
macOS, Linux and the iOS simulator. Fill caps are time-based today (`FILL_CAP`); on a slow phone
a cap can change which template fills, so the FFI takes the cap as an argument and the app
passes one sized for the device, or the fill moves to a step count.

**Licensing.** As §1. Output: a decision written into this document.

*Done when* an empty SwiftUI app on the simulator shows a crossword grid the Rust engine made.

## Phase 1: The app (L)

The core loop, by touch, with no AI and no Pencil.

- **Make.** Pick a puzzle type and size, pick a word list (bundled themes, or type your own
  words and clues), make. Same seed, same puzzle; "another" rolls a new seed.
- **Solve.** A crossword grid with a clue bar and the keyboard; a word search where you drag
  across letters; a ladder, a cryptogram, sudoku, nonogram and maze each with its own input.
  Check a letter, a word or the puzzle; reveal; undo.
- **Keep.** A library of made puzzles with progress, stored with SwiftData.
- **Print and share.** The books' PDF layout through AirPrint and the share sheet, so a puzzle
  goes on paper as it does from the desktop.
- **Accessibility from the start.** VoiceOver reads a square as "7 Across, letter 3 of 5,
  empty", Dynamic Type on clues, and nothing told by color alone.

*Done when* someone can make, solve and print a crossword and a word search on an iPhone and an
iPad, with VoiceOver.

## Phase 2: Apple Pencil (M–L)

The iPad's reason to exist for this app: solving on glass the way you solve on paper.

| Feature | How | Size | Hardware |
|---|---|---|---|
| **Write in the squares** | Each square takes Scribble through `UIIndirectScribbleInteraction`; one letter per square, the cursor moves on. A rebus square takes several. | M | Any Pencil |
| **Loop the words** | A PencilKit stroke over the word search; its ends snap to the nearest cells and a loose loop still finds the word. Found words stay circled in your own hand. | M | Any Pencil |
| **Pencil and pen** | Two inks: grey "pencil" for guesses, black "pen" for sure answers. Checking skips pencil marks unless asked. | S | Any Pencil |
| **Margin notes** | A PencilKit layer over the page for working out a cryptogram or a ladder, saved with the puzzle. | S | Any Pencil |
| **Hover** | Before the tip touches, the square under it highlights and its clue shows. | S | Pencil 2 or Pro on an M2-or-later iPad |
| **Squeeze** | Squeeze for the tool palette, or for a hint if you choose that in Settings. | S | Pencil Pro |
| **Barrel roll** | Rolling the Pencil turns the highlighter's angle in a word search. | S | Pencil Pro |
| **Haptic answer** | A tap in the Pencil when a word is right. | S | Pencil Pro |
| **Draw your own puzzle** | Sketch a nonogram picture on a grid, or the black squares of a crossword, and the engine fills the rest. | M | Any Pencil |

*Done when* a full 15×15 crossword and a word search can be solved with the Pencil alone,
and the Pencil Pro gestures each do something.

## Phase 3: Apple Intelligence (M) *AI*

The model contract from §1, in Swift.

- **"Make me a crossword about…"** A subject in, `@Generable` words and clues out, at the
  difficulty and style chosen (plain, kid, trivia, cryptic: the styles `ai.rs` has). Words are
  checked against the word list before the engine sees them; a clue containing its answer is
  dropped and asked for again; anything short is filled from the bundled list.
- **Rewrite the clues.** Any made crossword's clues in another style.
- **Routing.** Plain and kid go on-device. Trivia and cryptic try PCC, then fall back to
  on-device with a note that clues may be simpler. When the model isn't available, the button
  says why and offers the bundled themes.
- **Quality bar.** A fixed set of subjects run through each tier in a test target; the share of
  clues that pass the checks is tracked, so prompt changes can be judged.

*Done when* "a word search about the solar system for an 8-year-old" makes a good puzzle
offline on an iPhone 15 Pro, and the app still works on an iPhone 14.

## Phase 4: Siri, Shortcuts and the system (M) *partly AI*

App Intents, which also put the app's actions inside Apple Intelligence and Spotlight.

- **Make a puzzle** (subject, type, size, audience): "Hey Siri, make me a dinosaur word search
  for my kid." Returns the puzzle, or its PDF for a Shortcut to print or send.
- **Today's puzzle**: open, or start the daily one.
- **Print a puzzle book** for a subject: a Shortcut can make one every Sunday.
- **Visual Intelligence**: an intent that offers "Make a puzzle from this" on whatever the camera
  or a screenshot shows (needs phase 7's text extraction).
- **Focus filter**: a "Kids" focus shows only kid-style puzzles.

*Done when* each intent works from Siri, from Shortcuts and from Spotlight.

## Phase 5: Lock Screen and Home Screen (M)

- **Live Activity** for a solve in progress: the timer and "38 of 42 words" on the Lock Screen
  and in the Dynamic Island; it ends when the puzzle does. *S.*
- **Interactive widgets**: today's word ladder, playable a rung at a time; a small "today's mini"
  that opens the puzzle; a streak widget. Widgets redraw on a budget, so only the smallest
  puzzles are playable in place. *M.*
- **Control Center control** to make a quick puzzle. *S.*

*Done when* a solve shows live on the Lock Screen and the ladder can be finished from the
Home Screen.

## Phase 6: Together (M–L)

- **SharePlay co-op** (GroupActivities): two or more people solve the same grid over FaceTime or
  Messages, each with their own cursor color; letters sync as they're typed or written. The
  puzzle travels as its seed and word list, so every device builds the identical grid. *L.*
- **Race mode**: same puzzle, separate grids, first to finish wins. *S on top of co-op.*
- **Game Center**: daily-puzzle leaderboards by time, achievements (first cryptic, a 30-day
  streak), and challenges to friends, which also show in Apple's Games app. *M.*
- **Share a puzzle**: a link or a Messages bubble that carries the seed and words, so the
  recipient gets the same puzzle, not a picture of it. *S.*

*Done when* two iPads can finish a crossword together over FaceTime.

## Phase 7: The camera (L) *partly AI*

- **Photograph to puzzle** *AI*: photograph a menu, a recipe, a page of a textbook. Vision reads
  the text; the model (image input, PCC when on-device can't) picks the theme words and writes
  clues; the engine builds the word search or crossword. *M.*
- **Photograph to nonogram**: `nonogram::from_picture` already turns a picture into a nonogram;
  the camera just feeds it. *S. The cheapest wow in this roadmap.*
- **Scan a paper puzzle**: point the camera at a newspaper crossword. Vision's document
  recognition finds the grid, its black squares and numbers, and the clue lists, and the app
  makes it solvable on screen. Only for your own use: it never shares a scanned puzzle. *L.*
- **Check a printed puzzle**: print a Deckle puzzle, solve it in pen, photograph it; the app
  reads the letters in each square and marks what's wrong. The printed page carries a small code
  with the seed, so the app knows the answers. *M.*

*Done when* a photo of a recipe becomes a word search, and a photo of a printed and solved
Deckle crossword is checked correctly.

## Phase 8: Delight (S each) *some AI*

- **Haptics** (Core Haptics): a small tap per correct letter, a pattern when a word completes, a
  bigger one at the end.
- **Read aloud**: clues spoken for children or for hands-free solving, with the system voices.
- **Bilingual puzzles**: clues in one language, answers in another, for language learners, using
  Apple's Translation framework and a word list in the second language. *M, not S: each
  language needs its own word list and blocklist.*
- **Cover art** *AI*: Image Playground draws a cover for a themed puzzle book, where it's
  available.
- **Sounds and themes**: newsprint, chalkboard, dark; a pencil-scratch sound with the Pencil.

## Phase 9: Everywhere (M–L)

- **iCloud sync** with CloudKit: the library and progress on every device. *M.*
- **Handoff**: start a crossword on the iPhone, finish it with the Pencil on the iPad. *S once
  sync exists.*
- **Mac**: the SwiftUI app on the Mac. Whether this replaces the Puzzles tab in the Tauri app or
  sits beside it is decided then. *M.*
- **Apple Watch**: today's word ladder, a rung at a time with the Digital Crown. *M.*

## Phase 10: Experiments (L)

- **AR word search** (RealityKit, ARKit): the grid lies on the kitchen table and you find words
  by walking around it. A gimmick, but a good one for the App Store page and for children.
- **Vision Pro**: the same app with the grid floating in the room; follows from the iPad build.

## 3. Risks

- **Licensing** (§1) blocks the App Store until settled.
- **PCC terms**: the download cap counts every app the developer has ever shipped, and the daily
  limits are unpublished. Nothing may depend on PCC answering.
- **On-device clue quality**: a small model writes wrong facts and weak cryptics. The checks,
  the fallbacks and the quality set in phase 3 are the answer; cryptic may stay PCC-only.
- **Fill time on phones**: the time-capped fill could pick another template on a slow device
  (phase 0).
- **Scanning others' puzzles**: a scanned puzzle is for the person who scanned it; the app never
  publishes or shares one.

## 4. Status

| Phase | Status |
|---|---|
| 0 | Started: `deckle-puzzle` crate split out, builds for iOS, checked in CI. Next: UniFFI bindings, the determinism test, the licensing decision. |
| 1–10 | Not started. |
