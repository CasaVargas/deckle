# A model on this computer, without the setup — design

**Status:** draft for discussion, 2026-11-02 · **Owner:** Jonathan (CasaVargas LLC)
**Builds on:** `2026-10-18-deckle-design.md` (§7 LLM policy), `2026-10-24-daily-llm-editor-design.md`,
`2026-11-01-ai-extras-design.md` (§1, why no bundled model)

Deckle already works with a local model: X-Ray, chapter recaps and the Daily editor all accept
an OpenAI-compatible server such as Ollama. But getting there asks the owner to know what Ollama
is, install it, pull a model by its tag in a terminal, pick "OpenAI-compatible" from a menu, type
`http://localhost:11434/v1` and the tag into two free-text fields, leave an "API key" field
empty on purpose, and then send a whole book to find out whether any of it worked. This design
makes "an AI model on this computer" a first-class choice that sets itself up, with no model
bundled and no change to the privacy story.

## 1. What is intimidating today, exactly

1. **The words.** "OpenAI-compatible (Ollama, LM Studio…)", "Endpoint", "API key", "Model" with a
   `claude-sonnet-5-5` placeholder. Nothing on the page says "this one stays on your computer
   and is free".
2. **Two setups.** Books (Settings → Reading aids) and the Daily editor (Daily → The paper) each
   have their own provider, model and endpoint, joined by a "Use Books provider" button.
3. **No feedback.** Nothing tells the owner the model answers until a book or a paper is built.
4. **The Daily editor doesn't fit a local model.** X-Ray (batches of 25) and recaps (2,000-word
   parts) were sized for Ollama's default 4k context. The editor is one request of up to 160 KB
   (~40k tokens) with an 8,192-token answer. Ollama's OpenAI endpoint can't raise the context, so
   on a default install the prompt is silently cut and the owner gets "AI editor unavailable"
   with no cause, however carefully they set it up.

## 2. Decisions proposed

| Question | Proposal |
|---|---|
| Bundle a model or llama.cpp | No (unchanged, ai-extras §1). Ollama stays the runtime; Deckle installs nothing, downloads nothing itself, and only talks to a server the owner started. |
| How Deckle talks to Ollama | A third provider, `ollama`, on Ollama's **native API** (`/api/chat`, `/api/tags`, `/api/pull`, `/api/show`), not the OpenAI shim. That is what lets Deckle set the context size, turn thinking off properly, ask for JSON, list installed models and download one with progress. "OpenAI-compatible" stays for LM Studio, vLLM and other machines. |
| The default model | **`qwen3:4b`** (≈2.5 GB on disk, runs in 8 GB of RAM). On a machine with 16 GB or more, **`qwen3:8b`**, the model this repo has already measured for X-Ray and recaps. One constant, `RECOMMENDED_MODELS`, pinned and tested; not a free-text default. |
| One setting or two | One. Settings → AI holds the choice; the Daily editor uses it unless its `[llm]` table says otherwise (CLI and Docker users keep the table). |
| What "set up" means | Three checks the page runs itself: Ollama answers; the recommended model is installed; a one-line test prompt comes back. Each with its own fix button. |

## 3. The setting, as the owner sees it

A single **AI** page (today's "An AI model" fields move here; Reading aids and Daily link to it).
Three cards, one chosen:

- **Off.** X-Ray uses Deckle's own lists; no recaps; no editor. (Default, unchanged.)
- **On this computer.** "Free, private, slower. Runs on a small model through Ollama; nothing
  leaves this Mac." Under it, the three checks as a short checklist that fills itself in:
  1. *Ollama* — "Running" / "Not installed" with **Get Ollama** (opens ollama.com/download) /
     "Installed but not running" with how to start it. Deckle polls `GET /` every few seconds
     while the page is open, so the tick appears the moment the app launches.
  2. *Model* — "qwen3:4b, 2.5 GB, installed" / **Download (2.5 GB)**, a progress bar from
     `/api/pull`'s stream, cancellable. Picked by RAM; the card says why ("8 GB of memory on
     this Mac").
  3. *Try it* — sends a tiny fixed prompt and shows "Answered in 1.4 s" or the error in plain
     words ("Ollama answered, but the model isn't loaded yet; try again").
  Then one line of expectation: "A novel's recaps take about 20 minutes on this Mac and happen
  in the background; the morning paper's editor takes a minute or two."
  An **Advanced** disclosure: a dropdown of every installed model (from `/api/tags`), and an
  address field for Ollama on another machine.
- **A service online.** Anthropic or OpenAI-compatible, key, model, endpoint: today's fields,
  with the price table from ai-extras §1 beside them. Marked **Online**, as now.

Naming: "Deckle, on this Mac" (the heuristic) and "On this computer" (the model) must not read
as the same thing. Proposed labels on the X-Ray card: *Deckle's own lists* / *The AI model you
chose*, and the AI page says which that is.

The Online & privacy page keeps its rows; "On this computer" reads "Sends nothing anywhere".

## 4. Backend

**`deckle_xray::llm::Provider::Ollama`**, endpoint default `http://localhost:11434`.
`complete` and `describe` on this provider post to `/api/chat` with:

- `options.num_ctx` chosen per call: 8,192 for an X-Ray batch, 8,192 for a recap part, 32,768
  for the Daily editor (qwen3 runs 32k natively; the KV cache for it on a 4B model is well
  under 2 GB). Today's batch and part sizes stay as the safe floor.
- `think: false` (replaces the `reasoning_effort: "none"` guess-and-retry, which stays for the
  OpenAI-compatible path).
- `format`: a JSON schema for X-Ray's entity array and the editor's plan object, so a 4B model
  can't answer with prose or a fenced block. Recaps ask for plain text, no `format`.
- `keep_alive: "10m"` so the model stays loaded between a book's batches.
- `stream: false`; the timeout stays per request.

Also on this provider:

- `probe(endpoint) -> Status { reachable, version, models: Vec<(name, size)> }` from `GET /`
  and `/api/tags`.
- `pull(endpoint, model, progress: &mut dyn FnMut(done, total))` streaming `/api/pull`,
  surfaced as a Tauri command with events, and as `deckle llm pull`.
- `recommended_model(total_ram_bytes) -> &'static str`.
- `deckle llm check` (CLI) runs the three checks and prints them.

**Daily editor on a local model.** Besides the larger `num_ctx`, the local path trims the
request: excerpts bounded at 600 characters instead of 1,600 and at most 40 stories per request,
splitting the paper's sections across two requests when it has more. Duplicate groups are found
within a request only, which is the common case (the same story from two feeds lands in the
same request when sections are kept whole). A request that still doesn't fit falls back as
today, but the runner note names the cause ("the editor's prompt is too long for the local
model") instead of the generic line.

**Config.** `Settings.llm_provider` gains `"ollama"`; `llm_model` empty means the recommended
model. The digest's `[llm]` table gains `provider = "ollama"`; absent, the Daily editor reads the
desktop setting (the app writes it into the paper's config on save, as it does for other shared
settings), so the "Use Books provider" button goes.

**Docker.** `docs/docker.md` gets a compose example with an `ollama` service and
`endpoint = "http://ollama:11434"`; `deckle llm pull` runs in the container's setup.

## 5. Why qwen3:4b

The three jobs are: a JSON array of one-line descriptions with person/term kinds and aliases
(X-Ray); a JSON plan with story IDs, three-sentence briefs and 0/1/2 scores (editor); and 2–4
sentences of prose in the book's language (recaps). They need JSON that parses, names copied
exactly, and competent prose in 100+ languages more than they need reasoning. Qwen3 covers 119
languages, follows a JSON schema well under Ollama's structured output, and lets thinking be
switched off, which halves its output time. The repo's own measurements (`docs/formats/xray.md`,
ai-extras §1) were made with qwen3:8b; 4b is the same family at half the download, for the
8 GB machines a first-time owner is likely to have. Gemma 3 4B is the fallback if recap prose
turns out noticeably better in testing; Qwen3.5 4B is reported newer but is untested here.
Whatever is pinned must pass the X-Ray and recap fixtures before it ships, with a measured time
on an M-series Mac and on a plain Windows laptop quoted in the setting.

## 6. Increments

1. **Backend first:** `Provider::Ollama` on the native API, `num_ctx`, `think: false`, JSON
   schemas, `probe`, `pull`, the editor's local request budget, `deckle llm check` /
   `deckle llm pull`. Tests with an injected transport, as the other providers.
2. **The AI page:** the three cards and the self-filling checklist, download with progress,
   the Advanced disclosure; Reading aids and the Daily page point to it.
3. **One setting:** the Daily editor inherits it; RAM-based default; docs and the README's
   two sentences on local models rewritten around "On this computer".

Each increment ships on its own; after 1 the CLI and Docker paths already work end to end.

## 7. Non-goals

- Bundling or auto-installing Ollama or a model (the owner clicks Get Ollama and Download).
- Starting Ollama for the owner at 5:30 for the morning build; the Ollama app runs at login,
  and a build that can't reach it says so in the runner note.
- Anything that changes which text goes where: the same three uses, opt-in, off by default.
