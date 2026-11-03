// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import { amazonRemoveNote, authorsFromText, authorsText, editFrom, editLabel, editProblem } from "./editDetails";

describe("editing a book's details", () => {
  it("reads authors however they're typed", () => {
    expect(authorsFromText("Ann Author & Bo Writer")).toEqual(["Ann Author", "Bo Writer"]);
    expect(authorsFromText("Corey, James S. A.\n Second ; Third ")).toEqual(["Corey, James S. A.", "Second", "Third"]);
    expect(authorsText(["A", "B"])).toBe("A & B");
  });
  it("builds the edit and says what's wrong", () => {
    const e = editFrom(" Dune ", "Frank Herbert", "Dune", "1");
    expect(e).toEqual({ title: "Dune", authors: ["Frank Herbert"], series: { name: "Dune", index: 1 } });
    expect(editProblem(e)).toBeNull();
    expect(editFrom("x", "", "", "3").series).toBeNull();
    expect(editProblem(editFrom(" ", "", "", ""))).toBe("A book needs a title.");
    expect(editProblem(editFrom("x", "", "S", "two"))).toMatch(/number/);
    expect(editLabel(e)).toBe("Dune by Frank Herbert");
  });
  it("warns that Amazon keeps what it delivered", () => {
    expect(amazonRemoveNote({ kind: "amazon-own", class: "amazon-own" })).toMatch(/Manage Your Content/);
    expect(amazonRemoveNote({ kind: "sideloaded", class: "locked" })).toMatch(/sent to Kindle by email/);
    expect(amazonRemoveNote({ kind: "deckle", class: "current" })).toBeNull();
  });
});
