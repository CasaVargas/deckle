--[[
Runs deckle.koplugin/main.lua outside KOReader, against a live Deckle reader server, with the
KOReader modules it uses stood in for (HTTP by curl, SHA-256 by shasum). For checking the
plugin's whole fetch on a computer; not run in CI, since it needs a server.

  luajit harness.lua <server> <username> <password-md5> <folder> [format]

Prints what the plugin did, then runs a second fetch, which must find the paper already there.
]]
local here = arg[0]:match("(.*/)") or "./"
local server, user, key, folder, format = arg[1], arg[2], arg[3], arg[4], arg[5] or "epub"
assert(server and user and key and folder, "usage: harness.lua <server> <user> <md5> <folder> [format]")
local settings_dir = folder .. "/.settings"
os.execute(("mkdir -p %q"):format(settings_dir))

local function sh(cmd)
    local p = io.popen(cmd)
    local out = p:read("*a")
    p:close()
    return out
end

local said = {}
local function say(...) local t = table.concat({ ... }, " "); table.insert(said, t); print("  plugin: " .. t) end

package.preload["datastorage"] = function() return {
    getSettingsDir = function() return settings_dir end,
    getDataDir = function() return folder end,
} end
package.preload["device"] = function() return { home_dir = folder } end
package.preload["ui/widget/infomessage"] = function() return { new = function(_, o) return o end } end
package.preload["ui/widget/notification"] = function() return { notify = function(_, text) say("notify:", text) end } end
package.preload["ui/uimanager"] = function() return {
    show = function(_, w) say("show:", w.text) end,
    nextTick = function(_, f) f() end,
} end
package.preload["ui/network/manager"] = function() return {
    isConnected = function() return true end,
    runWhenConnected = function(_, f) f() end,
} end
package.preload["ui/widget/container/widgetcontainer"] = function() return {
    extend = function(_, t) t.__index = t; return setmetatable(t, {}) end,
} end
package.preload["apps/filemanager/filemanager"] = function() return { instance = { onRefresh = function() say("file browser refreshed") end } } end
package.preload["logger"] = function() return { warn = function(...) say("warn:", ...) end } end
package.preload["gettext"] = function() return function(s) return s end end
package.preload["ffi/util"] = function() return {
    template = function(s, ...) local a = { ... }; return (s:gsub("%%(%d)", function(i) return tostring(a[tonumber(i)]) end)) end,
    purgeDir = function(d) os.execute(("rm -rf %q"):format(d)) end,
} end
package.preload["util"] = function() return { makePath = function(p) os.execute(("mkdir -p %q"):format(p)) end } end
package.preload["libs/libkoreader-lfs"] = function() return {
    attributes = function(p, what)
        if what == "mode" then
            return sh(("test -d %q && echo d"):format(p)):match("d") and "directory" or nil
        end
        local f = io.open(p, "rb")
        if not f then return nil end
        local size = f:seek("end")
        f:close()
        return size
    end,
} end
package.preload["ffi/sha2"] = function() return {
    sha256 = function(bytes)
        local tmp = os.tmpname()
        local f = io.open(tmp, "wb"); f:write(bytes); f:close()
        local out = sh(("shasum -a 256 %q"):format(tmp)):match("^(%x+)")
        os.remove(tmp)
        return out
    end,
} end
-- A JSON object of strings and numbers: all the server's answer holds.
package.preload["json"] = function() return {
    decode = function(s)
        local t = {}
        for k, v in s:gmatch('"([%w_]+)"%s*:%s*("[^"]*"?)') do t[k] = v:sub(2, -2) end
        for k, v in s:gmatch('"([%w_]+)"%s*:%s*(%-?%d+%.?%d*)') do t[k] = tonumber(v) end
        return t
    end,
} end
package.preload["ltn12"] = function() return {
    sink = { table = function(t) return function(chunk) if chunk then table.insert(t, chunk) end return 1 end end },
} end
package.preload["socketutil"] = function() return {
    LARGE_BLOCK_TIMEOUT = 10, LARGE_TOTAL_TIMEOUT = 30,
    set_timeout = function() end, reset_timeout = function() end,
    file_sink = function(fh) return function(chunk) if chunk then fh:write(chunk) else fh:close() end return 1 end end,
} end
package.preload["socket"] = function() return { skip = function(_, ...) return select(2, ...) end } end
package.preload["socket.http"] = function() return {
    request = function(req)
        local args = {}
        for k, v in pairs(req.headers or {}) do table.insert(args, ("-H %q"):format(k .. ": " .. v)) end
        local tmp = os.tmpname()
        local code = tonumber(sh(("curl -s -o %q -w '%%{http_code}' %s %q"):format(tmp, table.concat(args, " "), req.url)))
        local f = io.open(tmp, "rb"); local body = f:read("*a"); f:close(); os.remove(tmp)
        if #body > 0 then req.sink(body) end
        req.sink(nil)
        return 1, code, {}, "HTTP " .. tostring(code)
    end,
} end
G_reader_settings = { readSetting = function() return nil end }

-- Deckle's install writes these settings; here the harness does.
local f = io.open(settings_dir .. "/deckle.lua", "w")
f:write(("return { server = %q, username = %q, key = %q, folder = %q, format = %q, keep = 2 }\n")
    :format(server, user, key, folder, format))
f:close()
-- A LuaSettings stand-in over that file.
package.preload["luasettings"] = function()
    local LS = {}
    LS.__index = LS
    function LS:open(path)
        local ok, t = pcall(dofile, path)
        return setmetatable({ path = path, data = ok and t or {} }, LS)
    end
    function LS:readSetting(k) return self.data[k] end
    function LS:saveSetting(k, v) self.data[k] = v end
    function LS:delSetting(k) self.data[k] = nil end
    function LS:nilOrTrue(k) return self.data[k] == nil or self.data[k] == true end
    function LS:isTrue(k) return self.data[k] == true end
    function LS:flush() end
    return LS
end

local Plugin = dofile(here .. "../deckle.koplugin/main.lua")
local p = setmetatable({ path = here .. "../deckle.koplugin", ui = { menu = { registerToMainMenu = function() end } } }, Plugin)
print("first fetch (as from the menu):")
p:init()
p:maybeFetch("menu")
print("second fetch:")
p:maybeFetch("menu")
print("an automatic fetch right after (waits ten minutes):")
p:maybeFetch("network")
print("fetched: " .. table.concat(p.settings:readSetting("fetched") or {}, ", "))
