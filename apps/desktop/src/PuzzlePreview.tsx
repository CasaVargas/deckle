// SPDX-License-Identifier: GPL-3.0-or-later
// The first puzzle of a book, drawn as the page will print it (Puzzles tab design §1): one
// SVG per kind, the theme answers tinted here and plain in ink.
import type { PuzzleDto } from "./types";

const INK = "var(--fg)";
const RULE = "var(--muted)";

function Grid({ n, cell, blacks }: { n: number; cell: number; blacks?: (r: number, c: number) => boolean }) {
  const side = n * cell;
  const lines = [];
  for (let k = 1; k < n; k++) lines.push(<line key={`v${k}`} x1={k * cell} y1={0} x2={k * cell} y2={side} stroke={RULE} strokeWidth={0.5} />, <line key={`h${k}`} x1={0} y1={k * cell} x2={side} y2={k * cell} stroke={RULE} strokeWidth={0.5} />);
  const fills = [];
  if (blacks) for (let r = 0; r < n; r++) for (let c = 0; c < n; c++) if (blacks(r, c)) fills.push(<rect key={`${r}-${c}`} x={c * cell} y={r * cell} width={cell} height={cell} fill={INK} />);
  return (
    <>
      {fills}
      {lines}
      <rect x={0} y={0} width={side} height={side} fill="none" stroke={INK} strokeWidth={1.5} />
    </>
  );
}

/** Crossword numbering, as the engine does it: a square starts an entry when it has no letter before it and one after. */
function numbers(rows: string[]): Map<string, number> {
  const out = new Map<string, number>();
  let n = 0;
  const open = (r: number, c: number) => r >= 0 && c >= 0 && r < rows.length && c < rows.length && rows[r][c] !== "#";
  for (let r = 0; r < rows.length; r++)
    for (let c = 0; c < rows.length; c++) {
      if (!open(r, c)) continue;
      const across = !open(r, c - 1) && open(r, c + 1);
      const down = !open(r - 1, c) && open(r + 1, c);
      if (across || down) out.set(`${r},${c}`, ++n);
    }
  return out;
}

export default function PuzzlePreview({ puzzle }: { puzzle: PuzzleDto }) {
  const W = 300;
  switch (puzzle.kind) {
    case "crossword": {
      const n = puzzle.rows.length;
      const cell = W / n;
      const nums = numbers(puzzle.rows);
      return (
        <div className="pz-preview">
          <svg viewBox={`0 0 ${W} ${W}`} role="img" aria-label={`A ${n} by ${n} crossword grid`}>
            <Grid n={n} cell={cell} blacks={(r, c) => puzzle.rows[r][c] === "#"} />
            {[...nums].map(([k, num]) => {
              const [r, c] = k.split(",").map(Number);
              return (
                <text key={k} x={c * cell + 1.5} y={r * cell + cell * 0.32} fontSize={Math.max(cell * 0.3, 5)} fill={INK}>
                  {num}
                </text>
              );
            })}
          </svg>
          <div className="pz-clues">
            {(["across", "down"] as const).map((dir) => (
              <div key={dir}>
                <strong>{dir === "across" ? "Across" : "Down"}</strong>
                {puzzle.clues
                  .filter((c) => c[1] === dir)
                  .map(([num, , clue, theme]) => (
                    <p key={`${dir}${num}`} className={theme ? "theme" : ""}>
                      <b>{num}</b> {clue}
                    </p>
                  ))}
              </div>
            ))}
          </div>
        </div>
      );
    }
    case "search": {
      const n = puzzle.rows.length;
      const cell = W / n;
      return (
        <div className="pz-preview">
          <svg viewBox={`0 0 ${W} ${W}`} role="img" aria-label={`A ${n} by ${n} word search`}>
            <Grid n={n} cell={cell} />
            {puzzle.rows.map((row, r) =>
              [...row].map((ch, c) => (
                <text key={`${r}-${c}`} x={c * cell + cell / 2} y={r * cell + cell * 0.7} fontSize={cell * 0.55} textAnchor="middle" fill={INK} fontFamily="ui-serif, Georgia, serif">
                  {ch}
                </text>
              )),
            )}
          </svg>
          <div className="pz-words">
            {puzzle.words.map(([w, theme]) => (
              <span key={w} className={theme ? "theme" : ""}>
                {w}
              </span>
            ))}
          </div>
        </div>
      );
    }
    case "sudoku": {
      const cell = W / 9;
      return (
        <div className="pz-preview">
          <svg viewBox={`0 0 ${W} ${W}`} role="img" aria-label="A sudoku">
            <Grid n={9} cell={cell} />
            {[3, 6].map((k) => (
              <g key={k}>
                <line x1={k * cell} y1={0} x2={k * cell} y2={W} stroke={INK} strokeWidth={1.5} />
                <line x1={0} y1={k * cell} x2={W} y2={k * cell} stroke={INK} strokeWidth={1.5} />
              </g>
            ))}
            {puzzle.givens.map((g, i) =>
              g ? (
                <text key={i} x={(i % 9) * cell + cell / 2} y={Math.floor(i / 9) * cell + cell * 0.72} fontSize={cell * 0.6} fontWeight={700} textAnchor="middle" fill={INK} fontFamily="ui-serif, Georgia, serif">
                  {g}
                </text>
              ) : null,
            )}
          </svg>
        </div>
      );
    }
    case "ladder": {
      const cell = 22;
      const cols = puzzle.ladders.length;
      const colW = W / cols;
      const tallest = Math.max(...puzzle.ladders.map((l) => l[2] + 1));
      const H = tallest * cell * 1.3 + 24;
      return (
        <div className="pz-preview">
          <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Three word ladders">
            {puzzle.ladders.map(([from, to, rungs], j) => {
              const x0 = j * colW + (colW - 4 * cell) / 2;
              return (
                <g key={j}>
                  <text x={j * colW + colW / 2} y={12} fontSize={10} textAnchor="middle" fill={INK} fontWeight={700}>
                    {from} to {to}
                  </text>
                  {Array.from({ length: rungs + 1 }, (_, k) => {
                    const y = 20 + k * cell * 1.3;
                    const word = k === 0 ? from : k === rungs ? to : "";
                    return (
                      <g key={k}>
                        {[0, 1, 2, 3].map((c) => (
                          <rect key={c} x={x0 + c * cell} y={y} width={cell} height={cell} fill="none" stroke={INK} strokeWidth={0.8} />
                        ))}
                        {[...word].map((ch, c) => (
                          <text key={c} x={x0 + c * cell + cell / 2} y={y + cell * 0.72} fontSize={cell * 0.6} textAnchor="middle" fill={INK} fontFamily="ui-serif, Georgia, serif">
                            {ch}
                          </text>
                        ))}
                      </g>
                    );
                  })}
                </g>
              );
            })}
          </svg>
        </div>
      );
    }
    case "cryptogram": {
      const given = new Map(puzzle.given);
      return (
        <div className="pz-preview">
          <div className="pz-crypt" aria-label="A cryptogram">
            {puzzle.words.map((w, i) => (
              <span className="pz-word" key={i}>
                {[...w].map((ch, k) =>
                  /[A-Z]/.test(ch) ? (
                    <span className="pz-letter" key={k}>
                      <em>{given.get(ch) ?? ""}</em>
                      <small>{ch}</small>
                    </span>
                  ) : (
                    <span className="pz-punct" key={k}>
                      {ch}
                    </span>
                  ),
                )}
              </span>
            ))}
          </div>
        </div>
      );
    }
    case "nonogram": {
      const n = puzzle.size;
      const rowW = Math.max(...puzzle.rows.map((r) => r.length));
      const colH = Math.max(...puzzle.cols.map((c) => c.length));
      const cell = W / (n + rowW * 0.7 + 0.3);
      const left = (rowW * 0.7 + 0.3) * cell;
      const top = (colH * 0.7 + 0.3) * cell;
      const fs = Math.max(Math.min(cell * 0.55, 11), 5);
      return (
        <div className="pz-preview">
          <svg viewBox={`0 0 ${W} ${top + n * cell + 2}`} role="img" aria-label={`A ${n} by ${n} nonogram`}>
            <g transform={`translate(${left}, ${top})`}>
              <Grid n={n} cell={cell} />
              {Array.from({ length: Math.floor((n - 1) / 5) }, (_, k) => (
                <g key={k}>
                  <line x1={(k + 1) * 5 * cell} y1={0} x2={(k + 1) * 5 * cell} y2={n * cell} stroke={INK} strokeWidth={1.2} />
                  <line x1={0} y1={(k + 1) * 5 * cell} x2={n * cell} y2={(k + 1) * 5 * cell} stroke={INK} strokeWidth={1.2} />
                </g>
              ))}
            </g>
            {puzzle.rows.map((clues, r) =>
              [...clues].reverse().map((v, k) => (
                <text key={`${r}-${k}`} x={left - 0.3 * cell - (k + 0.5) * 0.7 * cell} y={top + (r + 0.5) * cell + fs * 0.35} fontSize={fs} textAnchor="middle" fill={INK}>
                  {v}
                </text>
              )),
            )}
            {puzzle.cols.map((clues, c) =>
              [...clues].reverse().map((v, k) => (
                <text key={`${c}-${k}`} x={left + (c + 0.5) * cell} y={top - 0.3 * cell - (k + 0.5) * 0.7 * cell + fs * 0.35} fontSize={fs} textAnchor="middle" fill={INK}>
                  {v}
                </text>
              )),
            )}
          </svg>
        </div>
      );
    }
    case "maze": {
      const n = puzzle.size;
      const cell = W / n;
      const walls: React.ReactNode[] = [];
      for (let r = 0; r < n; r++)
        for (let c = 0; c < n; c++) {
          const w = puzzle.walls[r * n + c];
          const [x0, y0, x1, y1] = [c * cell, r * cell, (c + 1) * cell, (r + 1) * cell];
          if (w & 1) walls.push(<line key={`n${r}-${c}`} x1={x0} y1={y0} x2={x1} y2={y0} stroke={INK} strokeWidth={1} />);
          if (w & 2) walls.push(<line key={`e${r}-${c}`} x1={x1} y1={y0} x2={x1} y2={y1} stroke={INK} strokeWidth={1} />);
          if (w & 4) walls.push(<line key={`s${r}-${c}`} x1={x0} y1={y1} x2={x1} y2={y1} stroke={INK} strokeWidth={1} />);
          if (w & 8) walls.push(<line key={`w${r}-${c}`} x1={x0} y1={y0} x2={x0} y2={y1} stroke={INK} strokeWidth={1} />);
        }
      return (
        <div className="pz-preview">
          <svg viewBox={`-2 -14 ${W + 4} ${W + 28}`} role="img" aria-label={`A ${n} by ${n} maze`}>
            {walls}
            <text x={cell / 2} y={-4} fontSize={10} textAnchor="middle" fill={INK}>
              ↓
            </text>
            <text x={W - cell / 2} y={W + 11} fontSize={10} textAnchor="middle" fill={INK}>
              ↓
            </text>
          </svg>
        </div>
      );
    }
  }
}
