# Puzzle books, increment 3: sudoku book — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `deckle puzzles sudoku` builds a book of classic 9×9 sudoku, graded easy, medium
or hard, as a write-on PDF for the Kindle Scribe.

**Architecture:**
- **New `puzzle::sudoku`:**
  - a bitmask solver that counts solutions, up to 2;
  - a seeded full-grid generator;
  - a logical grader;
  - a digger that removes givens in 180°-symmetric pairs while the puzzle stays unique and
    within the difficulty's techniques.
- **Book:** the book model gains `Puzzles::Sudoku`.
- **PDF:** the renderer gets a sudoku page (one big grid with heavy 3×3 boxes) and
  answers at six per page.
- **CLI:** `deckle puzzles sudoku`, which has no `--size` option.

**Spec:** `docs/superpowers/specs/2026-11-01-puzzle-books-design.md` §3.4, §4.2–4.4, §7.

## Global Constraints

- **Uniqueness:** every puzzle has exactly one solution (the solver counts to 2).
- **Grades,** by the techniques a logical solver needs:
  - `easy`: naked and hidden singles only, with at least 36 givens *(builder's call)*.
  - `medium`: adds naked/hidden pairs and pointing/claiming, and needs at least one of them.
  - `hard`: can't be finished with the medium techniques, but is still unique.
- **Givens** are symmetric under 180° rotation *(builder's call: the classic look)*.
- **Determinism:** the same seed gives the same book. Attempts are counted, never timed.
- **Page:** cells of at least 40 pt. Givens are bold at 60% of the cell, leaving room for
  pencil marks.
  - Lines: 0.75 pt between cells, 2 pt for the box borders and the outline.
- **Answers:** six per page (2×3), givens bold, solved digits regular.
- **CLI:** `--size` isn't offered for sudoku. Book options that pass any size other than 9
  are refused.

## Review Focus

1. A `hard` request whose digging keeps ending at medium. A bounded retry must still produce
   a hard puzzle, or fail clearly, never loop.
2. The grader is sound: a puzzle graded easy really solves by singles alone, and so on.
   Check the grader's solution against the solver's.
3. Givens are never wrong: each one is the solution's digit.
4. The answers-page numbers when six answers share a page: `Answers on page N`.
5. Determinism across threads (a busy machine).

---

### Task 1: `puzzle::sudoku`

**Interfaces — produces:**
```rust
pub type Grid = [u8; 81];                         // 0 empty, 1–9
pub struct Sudoku { pub givens: Grid, pub solution: Grid }
pub enum Grade { Easy, Medium, Hard }
pub fn count_solutions(g: &Grid, limit: usize) -> usize
pub fn grade(g: &Grid) -> Grade                   // a unique puzzle's grade
pub fn make(grade: Grade, rng: &mut StdRng) -> Option<Sudoku>
```

**Steps:**
- [ ] **Tests:**
  - `a_full_grid_is_a_sudoku` (rows, columns and boxes each hold 1–9);
  - `count_finds_two_when_ambiguous`;
  - `grades_known_puzzles`: a singles-only puzzle is Easy, one that needs a pair or pointing
    is Medium, and a published hard one is Hard;
  - `every_grade_makes_unique_symmetric_puzzles` (20 seeds per grade: one solution, givens
    match the solution, symmetric, the grade matches, easy has ≥36 givens);
  - `deterministic`;
  - `a_busy_machine_makes_the_same_sudoku`.
- [ ] **Implement and commit:** `feat(puzzle): sudoku generator and grader`

### Task 2: Sudoku books (model + PDF)

**Model:**
- `Kind::Sudoku` with sizes `9..=9`;
- `Puzzles::Sudoku(Vec<Sudoku>)`;
- the default title "Sudoku";
- `Book::sudokus()`.

**PDF:**
- the sudoku page;
- answers at six per page, through a per-kind answers-per-page count;
- `pages()`.

**Steps:**
- [ ] **Tests:**
  - `a_sudoku_book_has_its_count`;
  - `sudoku_size_must_be_nine`;
  - PDF read-back:
    - the page count equals `pages()`;
    - page 2 shows "No. 1" and its givens;
    - "Answers on page N" is right for 7 puzzles (two answer pages);
    - same bytes from the same seed;
    - no annotations.
- [ ] **Implement and commit:** `feat(puzzle): sudoku books`

### Task 3: CLI, docs, review, PR

**Steps:**
- [ ] **CLI:** `PuzzlesCommand::Sudoku { difficulty, book }`.
- [ ] **CLI test:** `puzzles_sudoku_writes_a_pdf`.
- [ ] **Docs:** `docs/puzzle-books.md`, plus the spec where the build changes it.
- [ ] **Checks:** clippy, fmt and the focused suites.
- [ ] **Review:** a final reviewer on Opus.
- [ ] **PR:** stacked on #289.
- [ ] **Device:** send a sudoku book to the Scribe.
