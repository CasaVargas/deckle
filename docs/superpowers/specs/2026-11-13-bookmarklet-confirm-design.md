# Bookmarklets confirm on Deckle's own page

Date: 2026-11-13. Status: implemented.

## Why

A bookmarklet runs inside the page it is clicked on. Until now it carried the listener's token
in the URL it opened (`/save?token=…&url=…`), so a page could replace `window.open` before the
click, read the token, and from then on any page could add addresses to the reading queue or
link files to the Recipes inbox with a plain `GET /save`.

## Design

- The bookmarklets carry no token. **Save to paper** opens `/save?url=…&title=…`; **Save recipe**
  opens `/recipe#<recipe data>` (or `/save?kind=recipe&url=…` when the page has no recipe data).
- `GET /save` and `GET /recipe` save nothing. They answer with a page on the listener's own
  origin that shows what would be saved and a **Save** button. The page embeds the token; a page
  on another origin can open it but can't read it (the popup is cross-origin to the opener, and a
  cross-origin `fetch` gets no readable body).
- Saving is a `POST` from that page: `POST /save` (a form) or `POST /recipe` (JSON, as before).
  It needs the token and an `Origin` header that is the listener's own (`accepted_origins`).
  Browsers set `Origin` on a `POST`, and a page can't forge it.
- The click is the consent: a page can open the popup, but can't press its button.
- Bookmarklets made before this still work: the token in their URL is ignored, and the same
  confirmation page appears.

## Cost

One more click per save. The popup closes itself after saving, as before.

## Tests

- `GET /save` with a valid address queues nothing and shows a form with the token.
- `POST /save` with the token and Deckle's origin queues; without either, or with another origin,
  it is refused.
- `GET /recipe` without a token answers the popup, which saves only on the button.
- The bookmarklets and the install page contain no token.
