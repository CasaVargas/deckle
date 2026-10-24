# Optional Daily editor (#53)

The editor is off by default. A digest-local `[llm]` table selects the same Anthropic or
OpenAI-compatible providers, models, endpoints and environment-key conventions as X-Ray.
It contains no API key. Reuse X-Ray's HTTP/provider code with a generic completion entry
point; do not couple newspaper editing to X-Ray entity prompts.

One bounded completion per edition handles all three operations: cross-source duplicate
story groups, up to three short brief sentences per news section, and entertainment
horoscopes. Only Local/feed story titles and plain-text excerpts, section titles, horoscope
signs and the edition date enter the request. Saved reading, calendars, highlights, shelf
contents, credentials, pictures and source URLs are excluded. The desktop option explains
that this material goes to the configured provider. Enabling a particular source does not
implicitly enable the editor.

Stable IDs refer to the gathered stories. The model can suggest duplicate groups but cannot
supply replacement news articles. Keep the earliest story in each valid cross-section group;
never delete based on a title match or accept nonexistent/overlapping IDs. Briefs and
horoscopes are plain text, validated and escaped when rendered, visibly labelled AI-written.
News text remains unchanged. Horoscope failure retains the existing deterministic text.

Validate the complete answer before mutating the edition. Invalid JSON, invalid IDs,
truncation, oversized input/output, missing credentials or HTTP failure preserve the edition
and add one generic runner note. Never include raw provider errors, credentials or replies
in the paper or logs. The existing request timeout bounds the wait; no extra editor retry loop (the shared client may retry once when a local server rejects its reasoning field).
A rebuild may produce a different edited edition. There is no LLM cache in this increment.

Apply after gathering, before rendering/export so EPUB, PDF and edition bundles agree.
Use an injected completion transport and explicit key argument in tests. Tests must prove
that disabled editing never calls it, valid edits render, failures retain original content,
invalid references do not remove stories, generated text is escaped, personal sections
never enter prompts, and config saves retain the opt-in without storing credentials.

The new generic completion entry point uses `max_completion_tokens` for hosted OpenAI
and retains `max_tokens` for local compatible servers. The former includes reasoning
as well as visible output; truncated answers fall back without edits.
Source: [official OpenAI Chat Completions reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create).

Limits: 80 news stories, 20 news sections, 12 horoscope signs, 1,600 excerpt characters
per story, 160 KB prompt JSON, 8,192 output tokens and 100 KB returned text. Exceeding
these bounds falls back rather than silently editing only part of the paper. Briefs have
one to three sentences of at most 240 characters each; horoscopes at most 700 characters.
Only news sections get briefs; weather, puzzles, lottery, history and private sections keep
their original format. Config defaults are omitted when saving, and empty briefs are
omitted from the bundle, so editor-off editions retain their prior output.

The desktop can copy provider/model/endpoint from Books settings, but cannot copy the
stored key. Its opt-in and public-content disclosure appear in Daily → The paper. Users
must make the environment key available to the app or scheduler, or choose a local server.
No paid/live provider call is required by the test suite.
