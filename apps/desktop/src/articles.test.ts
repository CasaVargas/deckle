// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { parseAddresses } from "./articles";

test("one", () => expect(parseAddresses("https://a.com/x")).toEqual(["https://a.com/x"]));
test("several, deduped", () => expect(parseAddresses("https://a.com/x\nhttps://b.org https://a.com/x")).toEqual(["https://a.com/x", "https://b.org"]));
test("wrapped", () => expect(parseAddresses("see (https://a.com/x). and <https://b.org/y>")).toEqual(["https://a.com/x", "https://b.org/y"]));
test("junk", () => expect(parseAddresses("hello world")).toEqual([]));
test("not web", () => expect(parseAddresses("ftp://a.com file:///x")).toEqual([]));
test("keeps balanced parentheses", () => expect(parseAddresses("https://en.wikipedia.org/wiki/Mercury_(planet)")).toEqual(["https://en.wikipedia.org/wiki/Mercury_(planet)"]));
test("balanced parentheses in a sentence", () => expect(parseAddresses("see https://en.wikipedia.org/wiki/Mercury_(planet).")).toEqual(["https://en.wikipedia.org/wiki/Mercury_(planet)"]));
test("markdown link", () => expect(parseAddresses("[t](https://x.com/a)")).toEqual(["https://x.com/a"]));
