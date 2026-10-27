// SPDX-License-Identifier: GPL-3.0-or-later
//! The cookbook cover's ornament: a covered serving dish in the manner of an engraving, drawn
//! with canvas strokes. It is a fixed drawing, nothing in it is generated or random.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::cover::Canvas;

const INK: u8 = 0;
/// Half the dome's width at scale 1.0, in pixels.
const DOME_R: f32 = 190.0;
/// The dome's height above the rim at scale 1.0.
const DOME_H: f32 = 200.0;

/// Draws the dish with the middle of its plate at `(cx, cy)`. At `scale` 1.0 it is about 540
/// px wide and 330 px tall, `cy` being the underside of the plate.
pub(crate) fn draw(c: &mut Canvas, cx: f32, cy: f32, scale: f32) {
    let at = |u: f32, v: f32| (cx + u * scale, cy - v * scale);
    let stroke = |c: &mut Canvas, pts: &[(f32, f32)], w: f32| {
        for pair in pts.windows(2) {
            c.line(pair[0], pair[1], w * scale, INK);
        }
    };
    // A curve of an ellipse centred (u0, v0), radii (ru, rv), from angle a0 to a1.
    let arc = |u0: f32, v0: f32, ru: f32, rv: f32, a0: f32, a1: f32| -> Vec<(f32, f32)> {
        let n = 96;
        (0..=n)
            .map(|i| {
                let a = a0 + (a1 - a0) * i as f32 / n as f32;
                at(u0 + ru * a.cos(), v0 + rv * a.sin())
            })
            .collect()
    };

    // The plate: a flat ellipse seen from slightly above, its far rim fine and its near rim
    // heavy, with a second line under it for the foot.
    stroke(c, &arc(0.0, 34.0, 270.0, 30.0, 0.0, PI), 3.0);
    stroke(c, &arc(0.0, 34.0, 270.0, 30.0, PI, 2.0 * PI), 6.0);
    stroke(
        c,
        &arc(0.0, 22.0, 262.0, 24.0, PI + 0.12, 2.0 * PI - 0.12),
        2.5,
    );
    stroke(
        c,
        &arc(0.0, 8.0, 150.0, 12.0, PI + 0.05, 2.0 * PI - 0.05),
        4.0,
    );

    // The cover's band where it meets the plate.
    let rim = 52.0;
    stroke(c, &[at(-DOME_R - 18.0, rim), at(DOME_R + 18.0, rim)], 5.0);
    stroke(
        c,
        &[
            at(-DOME_R - 10.0, rim + 12.0),
            at(DOME_R + 10.0, rim + 12.0),
        ],
        2.5,
    );
    for end in [-1.0, 1.0] {
        stroke(
            c,
            &[
                at(end * (DOME_R + 18.0), rim),
                at(end * (DOME_R + 10.0), rim + 12.0),
            ],
            3.0,
        );
    }

    // The dome: a half ellipse.
    let top = rim + 12.0;
    stroke(c, &arc(0.0, top, DOME_R, DOME_H, 0.0, PI), 6.0);

    // The handle: a stem, a collar and a ball.
    let crown = top + DOME_H;
    stroke(c, &[at(-9.0, crown), at(-9.0, crown + 16.0)], 4.0);
    stroke(c, &[at(9.0, crown), at(9.0, crown + 16.0)], 4.0);
    stroke(c, &[at(-22.0, crown + 16.0), at(22.0, crown + 16.0)], 4.0);
    stroke(c, &arc(0.0, crown + 40.0, 20.0, 22.0, 0.0, 2.0 * PI), 4.0);
    stroke(
        c,
        &arc(-5.0, crown + 46.0, 8.0, 9.0, 0.6, FRAC_PI_2 + 1.2),
        2.0,
    );

    // Hatching on the shaded side, drawn along "/" lines and kept inside the dome. The
    // shade starts where `u - 0.35 v > 40` and a second, crossing set thickens it at the edge.
    let inside = |u: f32, v: f32, inset: f32| {
        let (ru, rv) = (DOME_R - inset, DOME_H - inset);
        v >= top + 8.0 && (u / ru).powi(2) + ((v - top) / rv).powi(2) <= 1.0
    };
    let mut k = -300.0_f32;
    while k < 300.0 {
        // The line v = top + (u - k) * 1.0, marched from left to right in 2 px steps.
        let mut run: Option<(f32, f32)> = None;
        let mut u = -DOME_R;
        while u <= DOME_R + 2.0 {
            let v = top + 10.0 + (u - k);
            let shaded = u - 0.35 * (v - top) > 40.0 && inside(u, v, 12.0);
            match (shaded, run) {
                (true, None) => run = Some((u, u)),
                (true, Some((a, _))) => run = Some((a, u)),
                (false, Some((a, b))) => {
                    let (v0, v1) = (top + 10.0 + (a - k), top + 10.0 + (b - k));
                    c.line(at(a, v0), at(b, v1), 2.0 * scale, INK);
                    run = None;
                }
                (false, None) => {}
            }
            u += 2.0;
        }
        if let Some((a, b)) = run {
            c.line(
                at(a, top + 10.0 + (a - k)),
                at(b, top + 10.0 + (b - k)),
                2.0 * scale,
                INK,
            );
        }
        k += 15.0;
    }
    // The crossing set, only toward the rim of the dome.
    let mut k = -100.0_f32;
    while k < 400.0 {
        let mut run: Option<(f32, f32)> = None;
        let mut u = 0.0;
        while u <= DOME_R + 2.0 {
            let v = top + 10.0 + (k - u);
            let shaded = u > 0.62 * DOME_R && inside(u, v, 12.0);
            match (shaded, run) {
                (true, None) => run = Some((u, u)),
                (true, Some((a, _))) => run = Some((a, u)),
                (false, Some((a, b))) => {
                    c.line(
                        at(a, top + 10.0 + (k - a)),
                        at(b, top + 10.0 + (k - b)),
                        1.6 * scale,
                        INK,
                    );
                    run = None;
                }
                (false, None) => {}
            }
            u += 2.0;
        }
        if let Some((a, b)) = run {
            c.line(
                at(a, top + 10.0 + (k - a)),
                at(b, top + 10.0 + (k - b)),
                1.6 * scale,
                INK,
            );
        }
        k += 15.0;
    }
    // The shine on the lit side: two short arcs inside the dome's outline.
    stroke(
        c,
        &arc(0.0, top, DOME_R - 22.0, DOME_H - 22.0, 2.15, 2.75),
        3.0,
    );
    stroke(
        c,
        &arc(0.0, top, DOME_R - 40.0, DOME_H - 40.0, 2.3, 2.62),
        2.0,
    );

    // Steam: three slow curls above the handle.
    for (i, du) in [-60.0_f32, 0.0, 60.0].into_iter().enumerate() {
        let base = crown + 90.0 + if i == 1 { 14.0 } else { 0.0 };
        let curl: Vec<(f32, f32)> = (0..=40)
            .map(|j| {
                let t = j as f32 / 40.0;
                at(du + 12.0 * (t * 2.0 * PI).sin(), base + t * 70.0)
            })
            .collect();
        stroke(c, &curl, 2.5);
    }
}
