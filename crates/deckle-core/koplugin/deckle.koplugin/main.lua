--[[--
Deckle for KOReader: fetches the morning paper from Deckle's reader server on your computer
when the reader wakes or joins Wi-Fi, or when you ask
(docs/superpowers/specs/2026-10-31-koreader-plugins-design.md §5).

Installed by `deckle koreader plugins install deckle`, which writes the server's address and
sign-in to settings/deckle.lua. It talks to nothing else.

@module koplugin.deckle
]]

local DataStorage = require("datastorage")
local ConfirmBox = require("ui/widget/confirmbox")
local Device = require("device")
local InfoMessage = require("ui/widget/infomessage")
local LuaSettings = require("luasettings")
local NetworkMgr = require("ui/network/manager")
local Notification = require("ui/widget/notification")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local ffiutil = require("ffi/util")
local json = require("json")
local lfs = require("libs/libkoreader-lfs")
local logger = require("logger")
local ltn12 = require("ltn12")
local sha2 = require("ffi/sha2")
local socket = require("socket")
local http = require("socket.http")
local socketutil = require("socketutil")
local util = require("util")
local _ = require("gettext")
local T = ffiutil.template

local Deckle = WidgetContainer:extend{
    name = "deckle",
    is_doc_only = false,
}

-- Pending plugin changes are applied once per KOReader start, by whichever instance comes first.
local pending_checked = false

function Deckle:init()
    -- By path: every plugin's folder is on package.path, so require("logic") could find another's.
    self.Logic = dofile(self.path .. "/logic.lua")
    self.settings = LuaSettings:open(DataStorage:getSettingsDir() .. "/deckle.lua")
    self.ui.menu:registerToMainMenu(self)
    if not pending_checked then
        pending_checked = true
        self:applyPending()
    end
    -- The file browser opening is a chance to fetch (after a book was closed, say).
    if not self.ui.document then
        UIManager:nextTick(function() self:maybeFetch("browser") end)
    end
end

--- Plugin changes Deckle left in deckle-pending.lua (design §5.4): applied to plugins_disabled,
--- which KOReader reads at start, so a restart finishes them. KOReader saves its settings on
--- exit, so they stick.
function Deckle:applyPending()
    local path = DataStorage:getDataDir() .. "/deckle-pending.lua"
    if lfs.attributes(path, "mode") ~= "file" then return end
    local ok, pending = pcall(dofile, path)
    os.remove(path)
    if not ok or type(pending) ~= "table" then
        logger.warn("Deckle: pending changes unreadable:", pending)
        return
    end
    local disabled, changed = self.Logic.apply_pending(
        G_reader_settings:readSetting("plugins_disabled"), pending.plugins)
    if changed == 0 then return end
    G_reader_settings:saveSetting("plugins_disabled", disabled)
    G_reader_settings:flush()
    UIManager:askForRestart(T(_("Deckle changed %1 plugin(s). Restart KOReader to finish."), changed))
end

--- Points KOReader's progress sync at Deckle, after asking (design §5.5). Auto-sync is left as
--- the owner set it.
function Deckle:useForSync()
    local server = self.settings:readSetting("server")
    local user = self.settings:readSetting("username")
    UIManager:show(ConfirmBox:new{
        text = T(_("Send progress sync to Deckle at %1, as %2?\n\nAutomatic sync stays as you set it in Progress sync."), server, user),
        ok_text = _("Use Deckle"),
        ok_callback = function()
            local key = self.settings:readSetting("key")
            local kosync = self.ui.kosync
            if kosync and kosync.settings then
                local new = self.Logic.sync_settings(kosync.settings, server, user, key)
                for k, v in pairs(new) do kosync.settings[k] = v end
                kosync.updated = true
                if kosync.onFlushSettings then kosync:onFlushSettings() end
            else
                local s = LuaSettings:open(DataStorage:getSettingsDir() .. "/kosync.lua")
                s:saveSetting("settings", self.Logic.sync_settings(s:readSetting("settings"), server, user, key))
                s:flush()
            end
            UIManager:show(InfoMessage:new{
                text = _("Progress sync now goes to Deckle. Turn on automatic sync in Tools → Progress sync if you want it."),
            })
        end,
    })
end

function Deckle:configured()
    return self.settings:readSetting("server") ~= nil and self.settings:readSetting("key") ~= nil
end

function Deckle:state()
    return {
        auto = self.settings:nilOrTrue("auto"),
        last_try = self.settings:readSetting("last_try"),
        username = self.settings:readSetting("username"),
        key = self.settings:readSetting("key"),
    }
end

--- Where papers go: the folder Deckle set, else Deckle/ in KOReader's home folder.
function Deckle:folder()
    local set = self.settings:readSetting("folder")
    if set and set ~= "" then return set end
    local home = G_reader_settings:readSetting("home_dir") or Device.home_dir or DataStorage:getDataDir()
    return home .. "/Deckle"
end

function Deckle:onResume() self:maybeFetch("resume") end
function Deckle:onNetworkConnected() self:maybeFetch("network") end

function Deckle:maybeFetch(trigger)
    if not self:configured() then return end
    local now = os.time()
    if not self.Logic.should_fetch(self:state(), trigger, now, self.ui.document ~= nil) then return end
    local connected = NetworkMgr:isConnected()
    -- Waking only turns Wi-Fi on when asked to; turning it on costs battery.
    if trigger ~= "menu" and not connected
        and not (trigger == "resume" and self.settings:isTrue("wifi_on_wake")) then
        return
    end
    self.settings:saveSetting("last_try", now)
    self.settings:flush()
    -- runWhenConnected, not runWhenOnline: Deckle is on the home network, and runWhenOnline wants
    -- the internet. It follows the owner's wifi_enable_action.
    NetworkMgr:runWhenConnected(function() self:fetch(trigger == "menu") end)
end

function Deckle:request(url, sink)
    socketutil:set_timeout(socketutil.LARGE_BLOCK_TIMEOUT, socketutil.LARGE_TOTAL_TIMEOUT)
    local code, headers, status = socket.skip(1, http.request{
        url = url,
        method = "GET",
        headers = self.Logic.headers(self:state()),
        sink = sink,
    })
    socketutil:reset_timeout()
    return code, headers, status
end

--- Asks for today's paper, fetches it unless it's here, checks it, and prunes the oldest.
function Deckle:fetch(verbose)
    local server = self.settings:readSetting("server")
    local format = self.settings:readSetting("format") or "epub"
    local body = {}
    local code = self:request(self.Logic.today_url(server, format), ltn12.sink.table(body))
    if code == 204 then return self:done(verbose, _("Deckle has no paper yet today.")) end
    if code ~= 200 then return self:failed(verbose, self.Logic.describe(code)) end
    local ok, answer = pcall(json.decode, table.concat(body))
    local today, why = self.Logic.check_today(ok and answer or nil)
    if not today then return self:failed(verbose, why) end

    local folder = self:folder()
    util.makePath(folder)
    local path = folder .. "/" .. today.file
    if self.Logic.have(lfs.attributes(path, "size"), today) then
        return self:done(verbose, _("Today's paper is already here."))
    end
    local part = folder .. "/." .. today.file .. ".part"
    local fh = io.open(part, "wb")
    if not fh then return self:failed(verbose, T(_("can't write to %1"), folder)) end
    code = self:request(self.Logic.absolute(server, today.url), socketutil.file_sink(fh))
    if code ~= 200 then
        os.remove(part)
        return self:failed(verbose, self.Logic.describe(code))
    end
    local f = io.open(part, "rb")
    local bytes = f and f:read("*a")
    if f then f:close() end
    if not bytes or #bytes ~= today.size or sha2.sha256(bytes) ~= today.sha256 then
        os.remove(part)
        return self:failed(verbose, _("the download didn't match what Deckle sent"))
    end
    os.remove(path) -- a FAT drive won't rename over a file
    if not os.rename(part, path) then
        os.remove(part)
        return self:failed(verbose, T(_("can't save %1"), path))
    end

    local fetched, drop = self.Logic.after_fetch(
        self.settings:readSetting("fetched"), path, self.settings:readSetting("keep") or 3)
    for _, old in ipairs(drop) do
        os.remove(old)
        local sdr = self.Logic.sidecar_dir(old)
        if lfs.attributes(sdr, "mode") == "directory" then ffiutil.purgeDir(sdr) end
    end
    self.settings:saveSetting("fetched", fetched)
    self.settings:saveSetting("last_ok", os.time())
    self.settings:delSetting("last_error")
    self.settings:flush()
    Notification:notify(T(_("Today's paper is here: %1"), today.file))
    local FileManager = require("apps/filemanager/filemanager")
    if FileManager.instance then FileManager.instance:onRefresh() end
end

function Deckle:done(verbose, text)
    if verbose then UIManager:show(InfoMessage:new{ text = text, timeout = 3 }) end
end

function Deckle:failed(verbose, why)
    logger.warn("Deckle: fetch failed:", why)
    self.settings:saveSetting("last_error", T(_("%1, at %2"), why, os.date("%H:%M")))
    self.settings:flush()
    if verbose then UIManager:show(InfoMessage:new{ text = T(_("Couldn't get the paper: %1"), why) }) end
end

function Deckle:addToMainMenu(menu_items)
    menu_items.deckle = {
        text = _("Deckle"),
        sorting_hint = "tools",
        sub_item_table = {
            {
                text = _("Get today's paper"),
                enabled_func = function() return self:configured() end,
                callback = function() self:maybeFetch("menu") end,
            },
            {
                text = _("Fetch automatically"),
                help_text = _("When the reader wakes with Wi-Fi on, or joins Wi-Fi, and no book is open."),
                checked_func = function() return self.settings:nilOrTrue("auto") end,
                callback = function()
                    self.settings:saveSetting("auto", not self.settings:nilOrTrue("auto"))
                    self.settings:flush()
                end,
            },
            {
                text = _("Turn Wi-Fi on when waking, to fetch"),
                help_text = _("Uses more battery. Follows your Wi-Fi setting for asking first."),
                checked_func = function() return self.settings:isTrue("wifi_on_wake") end,
                callback = function()
                    self.settings:saveSetting("wifi_on_wake", not self.settings:isTrue("wifi_on_wake"))
                    self.settings:flush()
                end,
            },
            {
                text = _("Use Deckle for progress sync"),
                enabled_func = function() return self:configured() end,
                callback = function() self:useForSync() end,
            },
            {
                text_func = function()
                    return T(_("Papers kept: %1"), self.settings:readSetting("keep") or 3)
                end,
                sub_item_table = (function()
                    local items = {}
                    for _, n in ipairs({ 1, 3, 7, 14 }) do
                        table.insert(items, {
                            text = tostring(n),
                            checked_func = function() return (self.settings:readSetting("keep") or 3) == n end,
                            callback = function()
                                self.settings:saveSetting("keep", n)
                                self.settings:flush()
                            end,
                        })
                    end
                    return items
                end)(),
            },
            {
                text_func = function()
                    local err = self.settings:readSetting("last_error")
                    local ok = self.settings:readSetting("last_ok")
                    if not self:configured() then return _("Not set up: install from Deckle on your computer") end
                    if err then return T(_("Last fetch: %1"), err) end
                    if ok then return T(_("Last paper: %1"), os.date("%a %H:%M", ok)) end
                    return _("No paper fetched yet")
                end,
                keep_menu_open = true,
                callback = function() end,
            },
        },
    }
end

return Deckle
