// SPDX-License-Identifier: GPL-3.0-or-later
import { describe, expect, it } from "vitest";
import type { PaperCard } from "./types";
import { anyKindleHint, daysText, deliveryLine, kindleLine, readerLine, slugFromTitle, typeLine } from "./papersView";

const usb: Pick<PaperCard, "at" | "days" | "email_to" | "usb_too"> = { at: "05:30", days: [], email_to: null, usb_too: false };

describe("Your papers", () => {
  it("says how and when each paper is delivered", () => {
    expect(deliveryLine(usb)).toBe("USB at 05:30");
    expect(deliveryLine({ ...usb, at: "07:00", days: ["sat"] })).toBe("USB, Saturdays at 07:00");
    expect(deliveryLine({ ...usb, at: "06:00", days: ["wed", "mon"] })).toBe("USB, Mon and Wed at 06:00");
    expect(deliveryLine({ ...usb, at: "06:00", email_to: "dad@kindle.com" })).toBe("Email at 06:00");
    expect(deliveryLine({ ...usb, at: "06:00", email_to: "dad@kindle.com", usb_too: true })).toBe("Email and USB at 06:00");
    expect(deliveryLine({ ...usb, at: null })).toBe("Not scheduled");
    expect(daysText(["mon", "tue", "wed", "thu", "fri", "sat", "sun"])).toBe("");
    expect(daysText(["fri", "mon", "wed"])).toBe("Mon, Wed and Fri");
  });

  it("names the Kindle a paper goes to", () => {
    const kindles = [{ sync_id: "ms:Kindle Scribe/G0001", name: "Mom's Kindle" }];
    expect(kindleLine({ kindle: "" }, kindles)).toBe("Any Kindle");
    expect(kindleLine({ kindle: "mom's kindle" }, kindles)).toBe("Mom's Kindle");
    expect(kindleLine({ kindle: "ms:Kindle Scribe/G0001" }, kindles)).toBe("Mom's Kindle");
    expect(kindleLine({ kindle: "Dad's Kindle" }, kindles)).toBe("Dad's Kindle (not named yet)");
  });

  it("gives the type, the reader and the any-Kindle hint", () => {
    expect(typeLine({ large_print: false })).toBe("Regular");
    expect(typeLine({ large_print: true })).toBe("Large print");
    expect(readerLine({ reader: " Maya " })).toBe("for Maya");
    expect(readerLine({ reader: "" })).toBe("");
    const main = { title: "The Morning Deckle", kindle: "" };
    expect(anyKindleHint([main])).toBeNull();
    expect(anyKindleHint([main, { title: "Kid Courier", kindle: "Maya's Kindle" }])).toBe(
      "The Morning Deckle goes to whichever Kindle connects first.",
    );
    expect(anyKindleHint([main, { title: "Kid Courier", kindle: "" }])).toBe(
      "The Morning Deckle and Kid Courier go to whichever Kindle connects first.",
    );
    expect(anyKindleHint([{ ...main, kindle: "Mom's Kindle" }, { title: "Kid Courier", kindle: "Maya's Kindle" }])).toBeNull();
  });

  it("makes the file name the way the backend does", () => {
    expect(slugFromTitle("The Kid Courier")).toBe("kid-courier");
    expect(slugFromTitle("The Lakeside Gazette!")).toBe("lakeside-gazette");
    expect(slugFromTitle("2026 Almanac")).toBe("almanac");
    expect(slugFromTitle("The")).toBe("the");
  });
});
