--[[
Runs KOReader's own calibre wireless client (plugins/calibre.koplugin: wireless.lua, metadata.lua,
extensions.lua, fetched by tools/fetch-koreader-calibre.sh) outside KOReader, with the KOReader
modules it uses stood in for. The socket is this process's stdin and stdout, framed as
"<length>\n<bytes>" each way; the Rust test (tests/koreader_wireless.rs) carries those to and
from Deckle's wireless server.

  luajit client.lua <koreader-calibre dir> <inbox> [password]
]]
local dir, inbox, password = arg[1], arg[2], arg[3]
assert(dir and inbox, "usage: client.lua <koreader-calibre dir> <inbox> [password]")

local function log(...)
    local t = {}
    for i = 1, select("#", ...) do t[#t + 1] = tostring((select(i, ...))) end
    io.stderr:write("koreader: ", table.concat(t, " "), "\n")
end

local function sh(cmd)
    local p = io.popen(cmd)
    local out = p:read("*a")
    p:close()
    return out
end

local function q(s) return "'" .. tostring(s):gsub("'", "'\\''") .. "'" end

-- JSON, as KOReader's rapidjson module offers it.
local null = setmetatable({}, { __tostring = function() return "null" end })
local ARRAY, OBJECT = { "array" }, { "object" }
local function array(t) return setmetatable(t or {}, { __jsontype = ARRAY }) end
local function object(t) return setmetatable(t or {}, { __jsontype = OBJECT }) end

local function is_array(t)
    local mt = getmetatable(t)
    if mt and mt.__jsontype == ARRAY then return true end
    if mt and mt.__jsontype == OBJECT then return false end
    local n = 0
    for _ in pairs(t) do n = n + 1 end
    return n > 0 and n == #t
end

local encode
local function encode_string(s)
    return '"' .. s:gsub('[%c"\\]', function(c)
        local map = { ['"'] = '\\"', ["\\"] = "\\\\", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
        return map[c] or string.format("\\u%04x", c:byte())
    end) .. '"'
end
function encode(v)
    local t = type(v)
    if v == null or v == nil then return "null"
    elseif t == "boolean" then return tostring(v)
    elseif t == "number" then
        if v == math.floor(v) and math.abs(v) < 2^53 then return string.format("%d", v) end
        return string.format("%.14g", v)
    elseif t == "string" then return encode_string(v)
    elseif t == "table" then
        local out = {}
        if is_array(v) then
            for _, x in ipairs(v) do out[#out + 1] = encode(x) end
            return "[" .. table.concat(out, ",") .. "]"
        end
        local keys = {}
        for k in pairs(v) do keys[#keys + 1] = tostring(k) end
        table.sort(keys)
        for _, k in ipairs(keys) do
            local x = v[k]
            if x == nil then x = v[tonumber(k)] end
            out[#out + 1] = encode_string(k) .. ":" .. encode(x)
        end
        return "{" .. table.concat(out, ",") .. "}"
    end
    error("can't encode " .. t)
end

local function decode(s)
    local i = 1
    local function ws() i = s:find("[^ \t\r\n]", i) or #s + 1 end
    local value
    local function str()
        i = i + 1
        local out = {}
        while true do
            local c = s:sub(i, i)
            if c == '"' then i = i + 1; break end
            if c == "\\" then
                local e = s:sub(i + 1, i + 1)
                if e == "u" then
                    local cp = tonumber(s:sub(i + 2, i + 5), 16)
                    i = i + 6
                    if cp >= 0xD800 and cp <= 0xDBFF and s:sub(i, i + 1) == "\\u" then
                        local lo = tonumber(s:sub(i + 2, i + 5), 16)
                        cp = 0x10000 + (cp - 0xD800) * 0x400 + (lo - 0xDC00)
                        i = i + 6
                    end
                    if cp < 0x80 then out[#out + 1] = string.char(cp)
                    elseif cp < 0x800 then out[#out + 1] = string.char(0xC0 + math.floor(cp / 64), 0x80 + cp % 64)
                    elseif cp < 0x10000 then out[#out + 1] = string.char(0xE0 + math.floor(cp / 4096), 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64)
                    else out[#out + 1] = string.char(0xF0 + math.floor(cp / 262144), 0x80 + math.floor(cp / 4096) % 64, 0x80 + math.floor(cp / 64) % 64, 0x80 + cp % 64) end
                else
                    local map = { n = "\n", r = "\r", t = "\t", b = "\b", f = "\f" }
                    out[#out + 1] = map[e] or e
                    i = i + 2
                end
            else
                out[#out + 1] = c
                i = i + 1
            end
        end
        return table.concat(out)
    end
    function value()
        ws()
        local c = s:sub(i, i)
        if c == "{" then
            local t = object()
            i = i + 1; ws()
            if s:sub(i, i) == "}" then i = i + 1; return t end
            while true do
                ws(); local k = str(); ws(); i = i + 1 -- :
                t[k] = value(); ws()
                local d = s:sub(i, i); i = i + 1
                if d == "}" then return t end
            end
        elseif c == "[" then
            local t = array()
            i = i + 1; ws()
            if s:sub(i, i) == "]" then i = i + 1; return t end
            while true do
                t[#t + 1] = value(); ws()
                local d = s:sub(i, i); i = i + 1
                if d == "]" then return t end
            end
        elseif c == '"' then return str()
        elseif s:sub(i, i + 3) == "true" then i = i + 4; return true
        elseif s:sub(i, i + 4) == "false" then i = i + 5; return false
        elseif s:sub(i, i + 3) == "null" then i = i + 4; return null
        else
            local num = s:match("^-?%d+%.?%d*[eE]?[-+]?%d*", i)
            i = i + #num
            return tonumber(num)
        end
    end
    local ok, v = pcall(value)
    if ok then return v end
    return nil, v
end

local function read_file(p)
    local f = io.open(p, "rb")
    if not f then return nil end
    local s = f:read("*a")
    f:close()
    return s
end

package.preload["rapidjson"] = function()
    return {
        null = null, array = array, object = object, decode = decode,
        encode = function(v) return encode(v) end,
        load = function(p) local s = read_file(p); if not s then return nil, "no file" end; return decode(s) end,
        load_calibre = function(p) local s = read_file(p); if not s then return nil, "no file" end; return decode(s) end,
        dump = function(v, p) local f = assert(io.open(p, "wb")); f:write(encode(v)); f:close(); return true end,
    }
end

local settings = { inbox_dir = inbox, calibre_wireless_password = password }
G_reader_settings = {
    readSetting = function(_, k) return settings[k] end,
    saveSetting = function(_, k, v) settings[k] = v end,
    delSetting = function(_, k) settings[k] = nil end,
}

local function mode(p)
    local kind = sh(("test -d %s && echo d || (test -f %s && echo f)"):format(q(p), q(p)))
    if kind:find("d") then return "directory" elseif kind:find("f") then return "file" end
end

package.preload["libs/libkoreader-lfs"] = function()
    return {
        attributes = function(p, what)
            local m = mode(p)
            if not m then return nil end
            local size = m == "file" and tonumber(sh("wc -c < " .. q(p)):match("%d+")) or 0
            local attr = { mode = m, size = size, modification = os.time() }
            if what then return attr[what] end
            return attr
        end,
    }
end
package.preload["logger"] = function() return { dbg = function() end, info = log, warn = log } end
package.preload["gettext"] = function() return function(s) return s end end
package.preload["ui/time"] = function() return { now = os.clock, since = function(t) return os.clock() - t end, to_ms = function(t) return t * 1000 end } end
package.preload["util"] = function()
    return {
        fileExists = function(p) return mode(p) == "file" end,
        arrayRemove = function(t, keep)
            local j = 1
            for i = 1, #t do
                if keep(t, i, j) then t[j] = t[i]; j = j + 1 end
            end
            for i = j, #t do t[i] = nil end
            return t
        end,
        diskUsage = function() return { available = 1000 * 1000 * 1000 } end,
        splitFilePathName = function(p)
            local d, n = p:match("^(.*/)([^/]*)$")
            return d or "", n or p
        end,
        makePath = function(p) os.execute("mkdir -p " .. q(p)) end,
        removeFile = function(p) return os.remove(p) end,
    }
end
package.preload["datastorage"] = function() return { getDataDir = function() return inbox .. "/.koreader" end } end
package.preload["ui/bidi"] = function() return { ltr = function(s) return s end, filepath = function(s) return s end } end
package.preload["ui/widget/confirmbox"] = function() return { new = function(_, o) return o end } end
package.preload["ui/widget/inputdialog"] = function() return { new = function(_, o) return o end } end
package.preload["ui/widget/infomessage"] = function() return { new = function(_, o) return o end } end
package.preload["device"] = function() return { model = "Kobo_frost", isAndroid = function() return false end } end
package.preload["ffi/util"] = function()
    return {
        template = function(s, ...) local a = { ... }; return (s:gsub("%%(%d)", function(n) return tostring(a[tonumber(n)]) end)) end,
        sleep = function() end,
        realpath = function(p) local r = sh("realpath " .. q(p) .. " 2>/dev/null"):gsub("%s+$", ""); return r ~= "" and r or nil end,
    }
end
package.preload["ffi/sha2"] = function()
    return {
        sha1 = function(s)
            local tmp = os.tmpname()
            local f = io.open(tmp, "wb"); f:write(s); f:close()
            local out = sh("(sha1sum " .. q(tmp) .. " 2>/dev/null || shasum " .. q(tmp) .. ") | cut -d' ' -f1"):gsub("%s+$", "")
            os.remove(tmp)
            return out
        end,
    }
end
package.preload["ffi/zeromq_h"] = function() return true end
package.preload["version"] = function() return { getCurrentRevision = function() return "v2026.09" end } end
package.preload["ui/network/manager"] = function() return { willRerunWhenConnected = function() return false end, isConnected = function() return true end } end
package.preload["readcollection"] = function() return { coll = {} } end
package.preload["ui/trapper"] = function() return { info = function() end, clear = function() end } end
package.preload["ui/uimanager"] = function()
    return {
        show = function(_, w) log("shows:", w.text) end,
        insertZMQ = function() end, removeZMQ = function() end,
        scheduleIn = function() end, unschedule = function() end,
    }
end
package.preload["ui/widget/container/widgetcontainer"] = function()
    return { extend = function(_, t) t.__index = t; return t end }
end
package.preload["apps/filemanager/filemanager"] = function() return {} end
package.preload["search"] = function() return { invalidateCache = function() end } end

-- The socket: what KOReader sends goes to stdout, framed.
package.preload["ui/message/streammessagequeue"] = function()
    return {
        new = function(_, o)
            o.start = function() end
            o.stop = function() end
            o.send = function(_, data)
                io.stdout:write(#data, "\n", data)
                io.stdout:flush()
            end
            return o
        end,
    }
end

package.path = dir .. "/?.lua;" .. package.path
local CalibreWireless = dofile(dir .. "/wireless.lua")
local CalibreMetadata = require("metadata")

os.execute("mkdir -p " .. q(inbox))
CalibreWireless:init()
CalibreWireless.re = function() end
CalibreWireless:initCalibreMQ("deckle", 9090)
CalibreMetadata:init(inbox)
log("connected; inbox " .. inbox)

while true do
    local line = io.stdin:read("*l")
    if not line then break end
    local n = tonumber(line)
    local data = n and n > 0 and io.stdin:read(n) or ""
    if data and #data > 0 then
        CalibreWireless.calibre_socket.receiveCallback({ data })
    end
end
log("the server closed the connection" .. (CalibreWireless.disconnected_by_server and " (ejected)" or ""))
if CalibreWireless.invalid_password then log("invalid password") end
