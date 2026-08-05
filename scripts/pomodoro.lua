-- Pomodoro -- an interactive focus timer, fully driven by keypresses.
--
-- Unlike wtfutil (where adding new interactive behavior means forking the
-- Go binary, wiring up a new widget struct, and recompiling), this
-- widget's entire behavior -- state machine, keybindings, and rendering --
-- lives in this single .lua file. Edit it, save, and slate picks it up
-- the next time the widget refreshes. No compiler, no rebuild, no restart.
--
-- Keys (while this widget is focused):
--   s - start / resume the countdown
--   p - pause the countdown
--   x - reset to a fresh work session
--
-- Usage: type = "lua:scripts/pomodoro.lua"
-- Settings: work_minutes = 25, break_minutes = 5

name = "Pomodoro"
description = "Interactive focus timer (start/pause/reset with s / p / x)"
version = "0.1.0"

local work_minutes = 25
local break_minutes = 5

-- Session state persists in the Lua VM for the lifetime of the widget --
-- across both refresh() and on_key() calls -- because slate keeps one
-- interpreter alive per widget instance. That's what makes keypresses
-- able to mutate state that later refreshes render.
local phase = "work" -- "work" or "break"
local running = false
local remaining = nil -- seconds left; initialized on first refresh()
local last_tick = nil -- os.time() of the last tick we accounted for

local function phase_seconds()
    return (phase == "work" and work_minutes or break_minutes) * 60
end

local function read_settings()
    if config_json then
        local w = config_json:match('"work_minutes"%s*:%s*(%d+)')
        if w then work_minutes = tonumber(w) end
        local b = config_json:match('"break_minutes"%s*:%s*(%d+)')
        if b then break_minutes = tonumber(b) end
    end
end

local function bar(fraction, width)
    width = width or 24
    local filled = math.floor(fraction * width + 0.5)
    if filled < 0 then filled = 0 end
    if filled > width then filled = width end
    return string.rep("\226\150\136", filled) .. string.rep("\226\150\145", width - filled)
end

local function format_time(secs)
    if secs < 0 then secs = 0 end
    local m = math.floor(secs / 60)
    local s = secs % 60
    return string.format("%02d:%02d", m, s)
end

local function advance_phase()
    phase = (phase == "work") and "break" or "work"
    remaining = phase_seconds()
    running = false
end

function refresh()
    read_settings()

    if remaining == nil then
        remaining = phase_seconds()
    end

    -- Account for real elapsed time since the last tick, so the countdown
    -- stays accurate no matter how often refresh() happens to run.
    local now = os.time()
    if running and last_tick then
        local elapsed = now - last_tick
        if elapsed > 0 then
            remaining = remaining - elapsed
        end
    end
    last_tick = now

    if remaining <= 0 then
        advance_phase()
    end

    local total = phase_seconds()
    local fraction = 1 - (remaining / total)
    local label = (phase == "work") and "\240\159\141\133 Focus" or "\226\152\149 Break"
    local status = running and "running" or "paused"

    local lines = {
        label .. " -- " .. status,
        bar(fraction) .. "  " .. format_time(remaining),
        "",
        "[s] start/resume  [p] pause  [x] reset",
    }

    return slate.text(table.concat(lines, "\n"), {wrap = false})
end

-- Called for every keypress the host doesn't handle itself (navigation,
-- tab, enter, etc. are reserved). This is the whole interactivity story:
-- a plain function that mutates local state based on which key came in.
function on_key(key, _action)
    if key == "Char('s')" then
        running = true
        last_tick = os.time()
    elseif key == "Char('p')" then
        running = false
    elseif key == "Char('x')" then
        phase = "work"
        remaining = phase_seconds()
        running = false
        last_tick = nil
    end
end
