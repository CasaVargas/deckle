-- Tests for deckle.koplugin/logic.lua on plain LuaJIT (KOReader plugins design §9):
--   luajit crates/deckle-core/koplugin/tests/logic_test.lua
local here = arg[0]:match("(.*/)") or "./"
local Logic = dofile(here .. "../deckle.koplugin/logic.lua")

local failures, count = 0, 0
local function test(name, f)
    count = count + 1
    local ok, err = pcall(f)
    if not ok then
        failures = failures + 1
        io.stderr:write("FAIL ", name, ": ", tostring(err), "\n")
    end
end
local function eq(a, b, what)
    if a ~= b then error((what or "") .. ": expected " .. tostring(b) .. ", got " .. tostring(a), 2) end
end
local function same(a, b, what)
    eq(#a, #b, (what or "") .. " length")
    for i = 1, #b do eq(a[i], b[i], (what or "") .. "[" .. i .. "]") end
end

test("the menu always fetches", function()
    eq(Logic.should_fetch({ auto = false, last_try = 1000 }, "menu", 1001, true), true)
end)

test("nothing automatic while a book is open, or with auto off", function()
    eq(Logic.should_fetch({ auto = true }, "resume", 10000, true), false)
    eq(Logic.should_fetch({ auto = false }, "network", 10000, false), false)
end)

test("automatic fetches wait ten minutes between tries", function()
    local s = { auto = true, last_try = 10000 }
    eq(Logic.should_fetch(s, "resume", 10000 + 599, false), false)
    eq(Logic.should_fetch(s, "network", 10000 + 600, false), true)
    eq(Logic.should_fetch({ auto = true }, "browser", 700, false), true, "never tried")
end)

test("addresses", function()
    eq(Logic.today_url("http://pc:47220/", "kepub"), "http://pc:47220/deckle/v1/today?format=kepub")
    eq(Logic.today_url("http://pc:47220", "epub"), "http://pc:47220/deckle/v1/today")
    eq(Logic.absolute("http://pc:47220/", "/opds/file/ab?format=kepub"), "http://pc:47220/opds/file/ab?format=kepub")
    eq(Logic.absolute("http://pc:47220", "https://elsewhere/x"), "https://elsewhere/x")
end)

test("the sign-in is the user and the MD5 key, never a password", function()
    local h = Logic.headers({ username = "deckle", key = "2ab96390c7dbe3439de74d0c9b0b1767" })
    eq(h["x-auth-user"], "deckle")
    eq(h["x-auth-key"], "2ab96390c7dbe3439de74d0c9b0b1767")
    eq(h["Authorization"], nil)
end)

local good = {
    file = "deckle-daily-2026-10-31.kepub.epub",
    size = 2358987,
    sha256 = string.rep("a1", 32),
    url = "/opds/file/abc?format=kepub",
}

test("a good answer passes", function()
    eq(Logic.check_today(good), good)
end)

test("bad answers are refused", function()
    local function with(k, v)
        local t = {}
        for kk, vv in pairs(good) do t[kk] = vv end
        t[k] = v
        return t
    end
    for _, bad in ipairs({
        with("file", "../../etc/passwd.epub"),
        with("file", "a/b.epub"),
        with("file", ".hidden.epub"),
        with("file", "paper.exe"),
        with("size", 0),
        with("size", "12"),
        with("sha256", "abc"),
        with("sha256", string.rep("zz", 32)),
        with("url", ""),
    }) do
        local t, why = Logic.check_today(bad)
        eq(t, nil, "refused " .. tostring(bad.file))
        assert(why, "says why")
    end
    eq(Logic.check_today(nil), nil)
end)

test("a paper of the same name and size is already here", function()
    eq(Logic.have(2358987, good), true)
    eq(Logic.have(100, good), false)
    eq(Logic.have(nil, good), false)
end)

test("the prune drops only papers it fetched, oldest first, beyond keep", function()
    local list, drop = Logic.after_fetch({ "/D/a.epub", "/D/b.epub", "/D/c.epub" }, "/D/d.epub", 3)
    same(list, { "/D/b.epub", "/D/c.epub", "/D/d.epub" }, "kept")
    same(drop, { "/D/a.epub" }, "dropped")
    -- fetching the same paper again moves it to the end, and drops nothing
    list, drop = Logic.after_fetch({ "/D/a.epub", "/D/b.epub" }, "/D/a.epub", 3)
    same(list, { "/D/b.epub", "/D/a.epub" })
    same(drop, {})
    -- keep is at least one, and a nonsense value means three
    list, drop = Logic.after_fetch({ "/D/a.epub" }, "/D/b.epub", 0)
    same(list, { "/D/b.epub" })
    list, drop = Logic.after_fetch(nil, "/D/a.epub", "lots")
    same(list, { "/D/a.epub" })
end)

test("a book's .sdr is its path less the last extension", function()
    eq(Logic.sidecar_dir("/mnt/onboard/Deckle/deckle-daily-2026-10-31.kepub.epub"),
        "/mnt/onboard/Deckle/deckle-daily-2026-10-31.kepub.sdr")
end)

test("failures read plainly", function()
    eq(Logic.describe(401):find("sign%-in") ~= nil, true)
    eq(Logic.describe(500), "Deckle answered 500")
    eq(Logic.describe("timeout"), "couldn't reach Deckle (timeout)")
end)

print(("%d tests, %d failed"):format(count, failures))
os.exit(failures == 0 and 0 or 1)
