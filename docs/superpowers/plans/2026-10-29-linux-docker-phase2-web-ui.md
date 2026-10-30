# Deckle on Linux, Phase 2: web UI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The Docker image serves Deckle's existing React app in a browser, backed by the same command code as the desktop app, behind one password, while the Phase 1 daemon keeps printing.

**Architecture:** The 106 `#[tauri::command]` bodies move out of `apps/desktop/src-tauri` into a new Tauri-free crate, `deckle-app`, which also exposes one `dispatch(name, args)` table. The desktop app keeps thin typed wrappers, so its IPC doesn't change. A new `deckle-server` crate (axum) serves the built frontend, `POST /api/invoke/<command>`, a Server-Sent Events stream for the four events, login, and a confined file browser and upload. `deckle serve` (deckle-cli, behind the `server` feature) runs that server and the Phase 1 daemon in one process. In the frontend, every `@tauri-apps/*` import moves behind `transport.ts` and `platform.ts`, which have a desktop and a web implementation picked at build time.

**Tech Stack:** Rust 2024 (axum 0.8, tokio, tower-http for static files; serde_json), React 19 + TypeScript + Vite 8 + vitest, pnpm, Docker.

**Spec:** `docs/superpowers/specs/2026-10-29-linux-docker-design.md`, "Phase 2 design". Phase 1 (merged in #236) supplies `deckle digest daemon`, `DECKLE_DAEMON`, `tz::process_tz()`, `secret::daemon_install()` and the image.

## Global Constraints

- The desktop app behaves exactly as before. Its frontend still calls the same command names over Tauri IPC, with the same argument and result JSON. No forked frontend: one `apps/desktop/src` tree builds both targets.
- The HTTP API mirrors Tauri. `POST /api/invoke/<snake_name>` takes a JSON body of the camelCase argument keys api.ts already sends. It returns `200` with the result JSON exactly as serde serialises it today, or `400 {"error": "<string>"}`. `401` means not logged in.
- Events: `GET /api/events` is SSE carrying `book-progress`, `run-finished`, `daily-finished` and `cookbook-illustrate`, with the payloads unchanged.
- Port `8080` by default (`DECKLE_WEB_PORT`), bound to `DECKLE_WEB_BIND` (default `0.0.0.0` in the image, `127.0.0.1` otherwise).
- Login uses one password from `DECKLE_WEB_PASSWORD`. If the password is unset and `DECKLE_WEB_AUTH` isn't `none`, the web UI doesn't start, the log says why, and the daemon keeps running. Sessions use a cookie that is `HttpOnly; SameSite=Strict; Path=/`. Every state-changing request must carry the header `X-Deckle: 1` (CSRF). There are no user accounts.
- The file browser and downloads are confined to the roots `/config`, `/data`, plus `DECKLE_WEB_ROOTS` (`:`-separated), after canonicalisation. Uploads go to `/data/Deckle/Uploads/<uuid>/<name>`, at most 512 MiB each.
- Capabilities are reported, not guessed. A feature that needs something missing (Previewer, USB, keychain, updater, reveal) stays visible with a one-line reason.
- No new outbound network calls. GPL-3.0 SPDX headers on new files. Match the repo's comment and naming style. Commit per task on `feat/web-ui`, never on `main`.

## Review Focus

1. **Password guessing.** A wrong password gets `401` after a fixed 1 s delay. The comparison is constant-time. Ten wrong tries in a row don't lock out the right password. Test: `login_wrong_password_is_slow_and_constant_time_safe` (Task 5).
2. **Escaping the roots.** `..`, absolute paths outside the roots, and a symlink inside `/data` pointing to `/etc` are all refused by list and download. Test: `fs_refuses_paths_outside_roots_including_symlinks` (Task 6).
3. **Server restart with a page open.** EventSource reconnects. The next invoke after the restart gets `401` (sessions are in memory), and the app shows the login screen instead of a broken page. Test: `web_transport_401_shows_login` (Task 9).
4. **A large upload, or one cancelled mid-way.** It streams to disk, never into memory. The 512 MiB limit gives `413`. A partial upload leaves no file. Test: `upload_over_limit_is_413_and_leaves_nothing` (Task 6).
5. **Running `deckle serve` with only the old Phase 1 environment** (no password set). The daemon still prints on schedule and the log names `DECKLE_WEB_PASSWORD`. A Phase 1 container that pulls the new image never crash-loops. Test: `serve_without_password_runs_daemon_only` (Task 7).

---

### Task 1: `deckle-app` crate and the first modules moved

**Files:**
- Create: `crates/deckle-app/{Cargo.toml,src/lib.rs,src/events.rs,src/state.rs}`
- Move: `apps/desktop/src-tauri/src/{morgue,next,readership,shelf}.rs` → `crates/deckle-app/src/`, along with their tests
- Modify: `Cargo.toml` (workspace member + `deckle-app` dependency), `apps/desktop/src-tauri/Cargo.toml`, `apps/desktop/src-tauri/src/lib.rs`, plus thin wrappers in a new `apps/desktop/src-tauri/src/wrap.rs`

**Interfaces:**
- Produces:
  ```rust
  // events.rs
  pub trait Events: Send + Sync { fn emit(&self, event: &str, payload: serde_json::Value); }
  pub struct NoEvents; // impl Events, does nothing (tests, CLI)
  // state.rs: AppState moves here unchanged (settings_file, devices_file, preload, preload_target, start_tab)
  pub struct AppState { /* same fields, pub */ }
  ```
  Each moved function keeps its name, parameters and return type, minus `#[tauri::command]`. `State<'_, AppState>` becomes `&AppState`, and `AppHandle` becomes `&dyn Events` (or `Arc<dyn Events>` where a thread outlives the call).
- The desktop gets a `struct TauriEvents(AppHandle)` implementing `Events` with `app.emit(event, payload)`, plus one-line `#[tauri::command]` wrappers with the original names and signatures.

- [ ] **Step 1:** Create the crate with no dependency on `tauri`. Move the four modules and their `#[cfg(test)]` tests. Keep the `*_at`/`*_with` helpers public inside the crate where wrappers need them.
- [ ] **Step 2:** Desktop `lib.rs`'s `generate_handler!` still lists the same command names, pointing at the wrappers in `wrap.rs`.
- [ ] **Step 3:** Verify:
  - `cargo nextest run -p deckle-app` passes, with the moved tests.
  - `cargo clippy -p deckle-app -p deckle-desktop --all-targets -- -D warnings` is clean.
  - CI is green on all three OSes.
- [ ] **Step 4:** Commit `refactor: move morgue, next, readership, shelf commands into deckle-app`.

### Task 2: Move books, kindle, upgrades, press and commands

**Files:** move `apps/desktop/src-tauri/src/{books,kindle,upgrades,press,commands}.rs` into `crates/deckle-app/src/`; add wrappers to `wrap.rs`.

**Interfaces:**
- Consumes: Task 1's `Events`, `AppState`.
- Produces:
  - `run_books(events: Arc<dyn Events>, state: &AppState, books: Vec<RunRequest>, target: RunTarget) -> Result<(), String>`. It still spawns its thread, emits `book-progress` per step and `run-finished` at the end, and returns at once.
  - `reveal(path)` stays here; the server marks it unsupported in Task 4.

- [ ] **Step 1:** Move the modules and tests. `run_books` emits `serde_json::to_value(&ProgressEvent)` and `serde_json::Value::Null` for `run-finished`.
- [ ] **Step 2:** Add a test in deckle-app, `run_books_emits_progress_then_finished`. It runs a recording `Events` impl and a `RunTarget::Folder` with one request whose path doesn't exist. That needs no Previewer, so it works in CI. Assert at least one `book-progress` arrives (with an error status), then exactly one `run-finished`, in that order.
- [ ] **Step 3:** Verify: clippy is clean, `deckle-app` tests pass, CI is green on all three OSes.
- [ ] **Step 4:** Commit `refactor: move book, kindle, upgrade, press and core commands into deckle-app`.

### Task 3: Move daily and cookbook. src-tauri becomes wrappers only

**Files:** move `apps/desktop/src-tauri/src/{daily,cookbook}.rs`; afterwards `apps/desktop/src-tauri/src/` holds only `main.rs`, `lib.rs` and `wrap.rs`.

**Interfaces:**
- Produces:
  - `daily_run(events: Arc<dyn Events>, exe: PathBuf, send: bool, paper: Option<String>)`. `exe` replaces the internal `current_exe()`. The desktop passes `current_exe()`; the server passes its own `current_exe()`, which is `deckle` (Task 7).
  - `morgue_send` and `press_setup` take `exe: &Path` the same way.
  - `cookbook_illustrate(events: Arc<dyn Events>, state: &AppState, id, all, redraw)`. The `ILLUSTRATING` and `STOP_ILLUSTRATING` statics move with it.

- [ ] **Step 1:** Move, thread `exe` through, and update the wrappers.
- [ ] **Step 2:** Add a test, `daily_run_emits_daily_finished`. Use a temporary script as `exe` that prints a fixed JSON report and exits 0 (on Windows, a `.cmd`; gate with `cfg` if needed and say why). Assert one `daily-finished` with `ok: true`.
- [ ] **Step 3:** Hand-check the desktop. Run `pnpm tauri dev` in `apps/desktop` on the Mac and open Books, Daily, Kindle and Cookbook; each loads its data. Run a Daily preview. Record what you saw in the report, with a screenshot of the Daily tab.
- [ ] **Step 4:** CI is green. Commit `refactor: daily and cookbook commands into deckle-app; desktop keeps only wrappers`.

### Task 4: The dispatch table, capabilities and web-mode behaviour

**Files:**
- Create: `crates/deckle-app/src/dispatch.rs`, `crates/deckle-app/src/capabilities.rs`
- Test: `crates/deckle-app/tests/it/{main.rs,dispatch.rs}`

**Interfaces:**
- Produces:
  ```rust
  pub struct Ctx { pub state: AppState, pub events: Arc<dyn Events>, pub exe: PathBuf, pub web: bool }
  pub fn dispatch(ctx: &Ctx, command: &str, args: serde_json::Value) -> Result<serde_json::Value, String>;
  pub const COMMANDS: &[&str]; // every command name, sorted
  #[derive(Serialize)] pub struct Capabilities { pub web: bool, pub previewer: bool, pub usb: bool, pub keychain: bool, pub updater: bool, pub reveal: bool, pub schedule: &'static str /* "install" | "config" */ }
  pub fn capabilities(web: bool) -> Capabilities; // also a command: "capabilities"
  ```
- `dispatch` deserialises `args` as a struct with `#[serde(rename_all = "camelCase")]` per command, which matches what Tauri does to argument names. It serialises the result with `serde_json::to_value`. Async commands are called with `tauri`-free blocking, the same way the bodies already work (`spawn_blocking` becomes a direct call; the server runs dispatch on `spawn_blocking`). Write the table with a `macro_rules!` so each command is one line.
- Web mode (`ctx.web`):
  - `reveal` and `morgue_open` return `Err("not available in the web app")`.
  - `email_set_password` and `subscription_set_password` return the Phase 1 "no keychain in the container: set …" message, using `secret::ENV` and `secret::site_env`.
  - `schedule_set` writes `[schedule] at` to config, like the daemon branch of `digest schedule`, and `schedule_get` reads it from config.
  - `take_preloaded_*` return empty, and `start_tab` returns `"daily"`.
- Add `capabilities` to the desktop wrappers too, with `web: false`.

- [ ] **Step 1: Failing tests:**
  - `every_api_ts_command_is_dispatchable`: parse `apps/desktop/src/api.ts` for `invoke<…>("name"` and assert the set equals `COMMANDS` (minus none). This guards against drift.
  - `dispatch_app_version_returns_string`.
  - `dispatch_unknown_command_errors`.
  - `dispatch_camel_case_args` (`set_device_options` with `cdeOverride` round-trips, or a simpler command with a camelCase arg such as `readership_apply`).
  - `web_mode_reveal_is_unavailable`.
  - `web_mode_password_set_names_env`.
  - `web_mode_schedule_set_writes_config` (temp config dir).
  - `capabilities_web_has_no_updater_or_keychain`.
- [ ] **Step 2:** Run `cargo nextest run -p deckle-app --test it dispatch` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** PASS. Clippy is clean.
- [ ] **Step 5:** Commit `feat(app): one dispatch table for every command, capabilities, web-mode behaviour`.

### Task 5: `deckle-server`: invoke, events, login, static files

**Files:**
- Create: `crates/deckle-server/{Cargo.toml,src/lib.rs,src/auth.rs,src/events.rs,src/invoke.rs,src/static_files.rs}`
- Test: `crates/deckle-server/tests/it/{main.rs,auth.rs,invoke.rs,events.rs}`

**Interfaces:**
- Consumes: `deckle_app::{Ctx, dispatch, Events}`.
- Produces:
  ```rust
  pub struct ServerConfig { pub bind: IpAddr, pub port: u16, pub password: Option<String>, pub auth_none: bool, pub web_dir: PathBuf, pub roots: Vec<PathBuf> }
  impl ServerConfig { pub fn from_env(web_dir: PathBuf) -> Result<ServerConfig, String>; } // Err when no password and not auth_none (message names DECKLE_WEB_PASSWORD)
  pub fn router(cfg: ServerConfig, ctx: Arc<Ctx>) -> axum::Router;
  pub async fn serve(cfg: ServerConfig, ctx: Arc<Ctx>, shutdown: impl Future<Output=()>) -> io::Result<()>;
  pub struct SseEvents; // impl deckle_app::Events, broadcasting to every open /api/events stream (tokio::sync::broadcast, capacity 256)
  ```
- Routes:
  - `POST /api/login {"password"}` sets the session cookie, or returns `401` after a 1 s delay.
  - `POST /api/logout`.
  - `GET /api/session` returns `{"auth": "password"|"none", "ok": bool}`.
  - `POST /api/invoke/:command` requires a session and `X-Deckle: 1`. It runs `dispatch` on `spawn_blocking`.
  - `GET /api/events` requires a session. It is SSE: `event: <name>`, `data: <json>`, with a keep-alive every 15 s.
  - Everything else serves `web_dir` with SPA fallback to `index.html`.
  - Security headers on every response: CSP `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:` (the desktop CSP), `X-Content-Type-Options: nosniff`, `Referrer-Policy: no-referrer`.
- Sessions: random 32-byte tokens (`getrandom`), held in memory, 30-day expiry. The password check uses `subtle::ConstantTimeEq`.

- [ ] **Step 1: Failing tests** (`tower::ServiceExt::oneshot`, temp dirs):
  - `invoke_requires_session` (401).
  - `invoke_requires_csrf_header` (403 without `X-Deckle`).
  - `login_then_invoke_app_version` (200, a JSON string).
  - `invoke_error_is_400_with_message`.
  - `login_wrong_password_is_slow_and_constant_time_safe`: wrong → 401 and ≥ 1 s; ten wrong tries, then the right one → 200.
  - `auth_none_needs_no_login`.
  - `from_env_without_password_errors_naming_var`.
  - `events_stream_delivers_emitted_event`: open SSE, call `SseEvents.emit("daily-finished", json)`, read the frame.
  - `static_fallback_serves_index`.
  - `security_headers_present`.
- [ ] **Step 2:** Run `cargo nextest run -p deckle-server` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** PASS. Clippy is clean.
- [ ] **Step 5:** Commit `feat(server): web API, events stream, login and static files`.

### Task 6: Files: browse, upload, download

**Files:** `crates/deckle-server/src/files.rs`; tests in `tests/it/files.rs`.

**Interfaces:**
- Routes (all require a session; POST also needs `X-Deckle`):
  - `GET /api/fs/roots` → `[{"path"}]`.
  - `GET /api/fs/list?path=` → `{"path", "parent": Option, "entries": [{"name", "path", "dir": bool, "size": u64}]}`, sorted with directories first.
  - `POST /api/fs/upload` (multipart, one or more files) → `["/data/Deckle/Uploads/<uuid>/<name>", …]`.
  - `GET /api/fs/download?path=` → the file, with `Content-Disposition: attachment`.
- `fn confine(roots: &[PathBuf], path: &Path) -> Result<PathBuf, StatusCode>` canonicalises and requires a canonical root prefix. A non-existent file in upload is never confined; uploads choose their own path. Upload file names are reduced to their last component, with `/`, `\` and leading dots stripped.
- Uploads stream to a temp file in the target folder, then rename. Over 512 MiB → `413`, and the temp file is deleted.

- [ ] **Step 1: Failing tests:**
  - `fs_lists_root_dirs_first`.
  - `fs_refuses_paths_outside_roots_including_symlinks` (`..`, `/etc`, a symlink to outside; skip the symlink case on Windows with a comment).
  - `upload_saves_under_uploads_and_returns_path`.
  - `upload_name_cannot_traverse` (`../../x.epub` → saved as `x.epub` inside its uuid folder).
  - `upload_over_limit_is_413_and_leaves_nothing` (lower the limit with a test-only constructor parameter).
  - `download_sets_attachment`.
- [ ] **Step 2–4:** RED, implement, GREEN. Clippy is clean.
- [ ] **Step 5:** Commit `feat(server): confined file browser, uploads and downloads`.

### Task 7: `deckle serve`

**Files:**
- Modify: `crates/deckle-cli/Cargo.toml` (`[features] server = ["dep:deckle-server", "dep:deckle-app", "dep:tokio"]`), `crates/deckle-cli/src/cli.rs` (`Command::Serve { #[arg(long)] port: Option<u16> }`, only with `#[cfg(feature = "server")]`), `crates/deckle-cli/src/lib.rs`
- Create: `crates/deckle-cli/src/serve.rs`
- Test: `crates/deckle-cli/tests/it/serve.rs` (compiled only with the feature; CI runs `cargo nextest run -p deckle-cli --features server serve::`)

**Interfaces:**
- `deckle serve`:
  1. Starts the Phase 1 daemon loop (`daemon::run`, from Task 4 of Phase 1) on its own thread, with its signal handling shared, so SIGTERM stops both.
  2. Builds `Ctx { state: AppState with settings/devices files under the XDG config dir, events: SseEvents, exe: current_exe(), web: true }`.
  3. Calls `ServerConfig::from_env(web_dir)`, where `web_dir` is `DECKLE_WEB_DIR` (default `/opt/deckle/web`). On `Err`, it logs `web UI off: <reason>` and keeps the daemon running. On `Ok`, it serves and logs `web UI on http://<bind>:<port>`.
- The desktop build doesn't enable the feature (check that `apps/desktop/src-tauri` doesn't turn it on).

- [ ] **Step 1: Failing tests:**
  - `serve_without_password_runs_daemon_only`: spawn `deckle serve` with `DECKLE_DAEMON_ONCE=1`, no password, a temp HOME and a paper due (reuse Phase 1 test setup). Assert exit 0, the edition printed, and stderr containing `web UI off` and `DECKLE_WEB_PASSWORD`.
  - `serve_with_password_answers_http`: spawn with `DECKLE_WEB_PASSWORD=pw`, `DECKLE_WEB_PORT=0` (bind an ephemeral port and log it), parse the port from stderr, then do a login and `invoke/app_version` with a small blocking HTTP client in the test (`ureq` dev-dependency). Kill it and assert a clean exit.
- [ ] **Step 2–4:** RED, implement, GREEN.
- [ ] **Step 5:** CI's rust job adds the `--features server` test run on Ubuntu only. Commit `feat(cli): deckle serve runs the web UI and the daemon together`.

### Task 8: Frontend platform layer (desktop behaviour unchanged)

**Files:**
- Create: `apps/desktop/src/transport.ts`, `apps/desktop/src/platform.ts`, `apps/desktop/src/platform/desktop.ts`
- Modify: `api.ts` and the 14 files that import `@tauri-apps/*`: App.tsx, YourDay.tsx, SettingsView.tsx, DailyView.tsx, EveryEdition.tsx, ExportFolders.tsx, SectionsTab.tsx, ThisKindle.tsx, Highlights.tsx, settings/HighlightsReading.tsx, settings/Originals.tsx, PaperSheet.tsx, WordsAnki.tsx, updates.ts, settings/About.tsx

**Interfaces:**
- ```ts
  // transport.ts
  export function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  export function listen<T>(event: string, cb: (payload: T) => void): Promise<() => void>;
  // platform.ts — the only other door to the host
  export interface Platform {
    kind: "desktop" | "web";
    pickFiles(o: { title?: string; extensions?: string[]; multiple?: boolean }): Promise<string[] | null>;
    pickFolder(o: { title?: string; defaultPath?: string }): Promise<string | null>;
    saveFile(o: { defaultName: string; extensions?: string[] }): Promise<string | null>; // a path the backend writes to
    afterSave(path: string): Promise<void>;          // desktop: nothing; web: download it
    openUrl(url: string): Promise<void>;
    homeDir(): Promise<string>;
    onFileDrop(h: { over(on: boolean): void; drop(paths: string[]): void }): Promise<() => void>;
    checkUpdate(): Promise<UpdateInfo | null>;        // existing updates.ts shape
    relaunch(): Promise<void>;
  }
  export const platform: Platform; // selected by import.meta.env.VITE_DECKLE_TARGET ("web" | default desktop)
  ```
- `api.ts` imports only `transport.ts` and `platform.ts`. Components import only `platform.ts`. After this task, `grep -r "@tauri-apps" apps/desktop/src` matches only `transport.ts` and `platform/desktop.ts`.
- Add `api.capabilities()`.

- [ ] **Step 1: Failing test** `platform_boundary.test.ts`: walk `src/` and assert that no file other than `transport.ts` and `platform/desktop.ts` imports `@tauri-apps`.
- [ ] **Step 2:** RED (`pnpm test`).
- [ ] **Step 3:** Move every call site behind `platform`. Keep each call's options (filters, titles, multiple, default paths) identical.
- [ ] **Step 4:** `pnpm build` and `pnpm test` are green. Run `pnpm tauri dev` and click every picker you touched once: the book picker, an export folder, a font, the `.ics` file and the Anki save. Each opens the same native dialog as before. Screenshot one in the report.
- [ ] **Step 5:** Commit `refactor(desktop): host calls go through transport and platform`.

### Task 9: Web implementation of the frontend

**Files:**
- Create: `apps/desktop/src/platform/web.ts`, `apps/desktop/src/transport/web.ts`, `apps/desktop/src/web/{Login.tsx,ServerPicker.tsx,Unavailable.tsx}`
- Modify: `transport.ts` (target switch), `package.json` (`"build:web": "tsc --noEmit && vite build --mode web --outDir dist-web"`), `vite.config.ts` (the web mode sets `VITE_DECKLE_TARGET=web`), and the components that host gated features, which read `api.capabilities()` and render `<Unavailable reason=…/>`
- Test: `apps/desktop/src/web/*.test.ts`

**Interfaces:**
- Web transport:
  - `invoke` sends `fetch("/api/invoke/"+cmd, {method: "POST", headers: {"Content-Type": "application/json", "X-Deckle": "1"}, credentials: "same-origin", body})`. A `400` throws the `error` string, just as Tauri rejects. A `401` emits `session-lost` (an in-app event) and throws.
  - `listen` uses one shared `EventSource("/api/events")` with a per-event fan-out.
  - `App.tsx` shows `<Login/>` while `/api/session` says not ok, or after `session-lost`.
- Web platform:
  - `pickFiles` opens `ServerPicker` in file mode, with an "Upload from this computer" button (`<input type=file multiple accept=…>` → `/api/fs/upload` → returned paths).
  - `pickFolder` opens `ServerPicker` in folder mode, starting at `defaultPath` or the first root.
  - `saveFile` returns `/data/Deckle/Exports/<defaultName>`; `afterSave(path)` opens `/api/fs/download?path=`.
  - `openUrl` → `window.open(url, "_blank", "noopener")`.
  - `homeDir` → `"/data"`.
  - `onFileDrop` uses HTML5 drag events on `document`; dropped files are uploaded, then `drop(paths)` is called.
  - `checkUpdate` → `null`; `relaunch` → no-op.
- Gating copy, exact:
  - "Kindle book conversion needs Kindle Previewer, which doesn't run on Linux."
  - "No e-reader is connected to this server."
  - "Passwords come from the server's environment: set DECKLE_SMTP_PASSWORD." (or the site's variable)
  - "Show in folder isn't available in the web app."
  - Updates: "Updates come with a new image tag."

- [ ] **Step 1: Failing tests** (vitest, mocking `fetch` and `EventSource`):
  - `web_invoke_posts_with_csrf_header_and_returns_json`.
  - `web_invoke_400_rejects_with_message`.
  - `web_transport_401_shows_login` (session-lost fires).
  - `web_listen_fans_out_one_eventsource`.
  - `server_picker_lists_dirs_first_and_climbs_to_parent_not_above_root` (a pure view-model function, so it needs no DOM).
- [ ] **Step 2–4:** RED, implement, GREEN. `pnpm build` and `pnpm build:web` both succeed.
- [ ] **Step 5: Visual check** (required; "it compiles" isn't done):
  - Build `deckle` with `--features server` on the Mac and run `DECKLE_DAEMON=1 DECKLE_WEB_PASSWORD=test DECKLE_WEB_DIR=apps/desktop/dist-web deckle serve`, with a temp `XDG_CONFIG_HOME` and `HOME`.
  - With Playwright, log in. Screenshot the Daily tab, the Books tab with the Previewer note, the server picker in folder mode, and Settings (with no updater shown).
  - Check the browser console has no errors. Put the screenshot paths in the report.
- [ ] **Step 6:** Commit `feat(web): browser transport, login, server file picker and capability gating`.

### Task 10: Image, CI and docs

**Files:**
- Modify: `docker/Dockerfile` (a `node:22` + pnpm stage running `pnpm install --frozen-lockfile && pnpm build:web`; copy `dist-web` to `/opt/deckle/web`; build `deckle` with `--features server`; `EXPOSE 8080`; `CMD ["deckle","serve"]`; `ENV DECKLE_WEB_BIND=0.0.0.0`)
- Modify: `tools/docker-smoke.sh`, `.github/workflows/ci.yml` (docker filter also watches `apps/desktop/src/**`), `docker/compose.example.yml` (port 8080 and `DECKLE_WEB_PASSWORD`), `docker/unraid/deckle.xml` (`WebUI` `http://[IP]:[PORT:8080]/`, port 8080, masked `DECKLE_WEB_PASSWORD`), `docs/docker.md` (a "The web app" section: password, `DECKLE_WEB_AUTH=none` only behind an authenticating proxy, what's gated and why, the file picker roots and `DECKLE_WEB_ROOTS`, uploads location), `README.md` (one line), `CHANGELOG.md`

- [ ] **Step 1:** Extend the smoke test:
  - Start with `DECKLE_WEB_PASSWORD=pw`. Wait for `web UI on`. Log in with curl, keeping the cookie jar. `invoke/app_version` returns the version. `invoke/daily_state` returns 200. `GET /` returns HTML. `GET /api/invoke/app_version` without the cookie returns 401.
  - Start without a password. The log says `web UI off` and the daemon still prints (reuse the existing print check).
- [ ] **Step 2:** CI is green, including the docker job. Commit `feat(docker): the image serves the web UI on 8080`.

---

## Self-review notes

- **Spec coverage:**
  - Shared layer: Tasks 1–4.
  - Transports: Tasks 8–9.
  - Server: Tasks 5–7.
  - Login: Task 5.
  - Pickers and upload: Tasks 6 and 9.
  - Open URL and reveal: Tasks 4 and 9.
  - Updater: Tasks 8–9.
  - Keychain and schedule: Task 4.
  - Capability gating: Tasks 4 and 9.
  - UI-off mode: Task 7.
  - Image and docs: Task 10.
- **USB passthrough** in the container is reported through `capabilities.usb` (`list_devices` non-empty). No extra work; the docs say it's best-effort.
