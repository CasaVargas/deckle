// SPDX-License-Identifier: GPL-3.0-or-later
import { expect, test } from "vitest";
import { parseAddresses } from "./articles";

test("one", () => expect(parseAddresses("https://a.com/x")).toEqual(["https://a.com/x"]));
test("several, deduped", () => expect(parseAddresses("https://a.com/x\nhttps://b.org https://a.com/x")).toEqual(["https://a.com/x", "https://b.org"]));
test("wrapped", () => expect(parseAddresses("see (https://a.com/x). and <https://b.org/y>")).toEqual(["https://a.com/x", "https://b.org/y"]));
test("junk", () => expect(parseAddresses("hello world")).toEqual([]));
test("not web", () => expect(parseAddresses("ftp://a.com file:///x")).toEqual([]));
