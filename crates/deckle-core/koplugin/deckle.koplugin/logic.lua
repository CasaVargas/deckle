--[[--
Deckle for KOReader: the decisions, with nothing of KOReader in them, so they're tested on plain
LuaJIT (docs/superpowers/specs/2026-10-31-koreader-plugins-design.md §5, §9).
]]

local Logic = {}

--- Seconds between two automatic fetches: waking and Wi-Fi often come together.
Logic.MIN_GAP = 600

--- Whether to fetch now. The menu always does; anything automatic waits until no book is open,
--- `auto` is on, and the last try was `MIN_GAP` ago.
-- @tparam table state `auto`, `last_try` (seconds)
-- @tparam string trigger `menu`, `resume`, `network` or `browser`
function Logic.should_fetch(state, trigger, now, reading)
    if trigger == "menu" then return true end
    if reading then return false end
    if state.auto == false then return false end
    return now - (state.last_try or 0) >= Logic.MIN_GAP
end

local function base(server)
    return (server or ""):gsub("/+$", "")
end

--- `GET` for today's paper, in `format` (`kepub` or `epub`).
function Logic.today_url(server, format)
    return base(server) .. "/deckle/v1/today" .. (format == "kepub" and "?format=kepub" or "")
end

--- `url` from the server's answer (a path) made absolute.
function Logic.absolute(server, url)
    if url:match("^https?://") then return url end
    return base(server) .. url
end

--- The headers every request carries: KOReader sync's sign-in pair, so the plugin never holds
--- the password.
function Logic.headers(state)
    return {
        ["x-auth-user"] = state.username or "",
        ["x-auth-key"] = state.key or "",
        ["Accept"] = "application/json",
        ["Accept-Encoding"] = "identity",
    }
end

--- The server's answer, checked: a bare `.epub` file name, a size, a SHA-256 and a URL.
-- @return the table, or nil and why
function Logic.check_today(t)
    if type(t) ~= "table" then return nil, "no answer" end
    local f = t.file
    if type(f) ~= "string" or f == "" or f:find("[/\\]") or f:sub(1, 1) == "."
        or not f:match("%.epub$") then
        return nil, "a file name Deckle wouldn't send"
    end
    if type(t.size) ~= "number" or t.size <= 0 then return nil, "no size" end
    if type(t.sha256) ~= "string" or #t.sha256 ~= 64 or not t.sha256:match("^%x+$") then
        return nil, "no checksum"
    end
    if type(t.url) ~= "string" or t.url == "" then return nil, "no address to fetch" end
    return t
end

--- Whether the paper is already here: a file of that name and size.
function Logic.have(existing_size, today)
    return existing_size ~= nil and existing_size == today.size
end

--- After fetching `path`: the list of papers this plugin fetched, newest last, and those beyond
--- `keep` to remove, oldest first. Only papers it fetched are ever in the list.
function Logic.after_fetch(fetched, path, keep)
    local list = {}
    for _, p in ipairs(fetched or {}) do
        if p ~= path then table.insert(list, p) end
    end
    table.insert(list, path)
    keep = math.max(1, math.floor(tonumber(keep) or 3))
    local drop = {}
    while #list > keep do
        table.insert(drop, table.remove(list, 1))
    end
    return list, drop
end

--- The folder KOReader keeps a book's settings in: its path less the last extension, `.sdr`
--- (docsettings.lua `getSidecarDir`).
function Logic.sidecar_dir(path)
    return (path:gsub("%.[^./]+$", "")) .. ".sdr"
end

--- KOReader's `plugins_disabled` after Deckle's pending changes (`wanted`: name → on). Returns
--- the new table and how many plugins changed.
function Logic.apply_pending(disabled, wanted)
    local out, changed = {}, 0
    for name, off in pairs(disabled or {}) do out[name] = off end
    for name, on in pairs(wanted or {}) do
        if type(name) == "string" and type(on) == "boolean" then
            local was_off = out[name] == true
            if on then out[name] = nil else out[name] = true end
            if was_off == on then changed = changed + 1 end
        end
    end
    return out, changed
end

--- Deckle's collection changes (deckle-collections.lua, calibre bridge design §6.2) as steps,
--- in the order to make them: collections deleted, made, books taken off, books added. Anything
--- that isn't a name or a list of absolute paths is left out.
function Logic.collection_ops(t)
    local ops = {}
    if type(t) ~= "table" then return ops end
    local function names(list, op)
        for _, name in ipairs(type(list) == "table" and list or {}) do
            if type(name) == "string" and name ~= "" then table.insert(ops, { op = op, name = name }) end
        end
    end
    local function books(map, op)
        local sorted = {}
        for name in pairs(type(map) == "table" and map or {}) do
            if type(name) == "string" and name ~= "" then table.insert(sorted, name) end
        end
        table.sort(sorted)
        for _, name in ipairs(sorted) do
            local files = map[name]
            for _, file in ipairs(type(files) == "table" and files or {}) do
                if type(file) == "string" and file:sub(1, 1) == "/" then
                    table.insert(ops, { op = op, name = name, file = file })
                end
            end
        end
    end
    names(t.delete, "delete")
    names(t.create, "create")
    books(t.remove, "remove")
    books(t.add, "add")
    return ops
end

--- The progress sync plugin's settings pointed at Deckle: its server, username and key (the
--- password's MD5, as kosync keeps it). Everything else, auto-sync included, stays as it was.
function Logic.sync_settings(current, server, username, key)
    local out = {}
    for k, v in pairs(current or {}) do out[k] = v end
    out.custom_server = (server or ""):gsub("/+$", "")
    out.username = username
    out.userkey = key
    return out
end

--- A one-line account of a failed fetch, for the menu.
function Logic.describe(code, why)
    if code == 401 then return "Deckle didn't accept the sign-in: install the plugin again from Deckle" end
    if type(code) == "number" then return "Deckle answered " .. code end
    if why then return why end
    return "couldn't reach Deckle (" .. tostring(code) .. ")"
end

return Logic
