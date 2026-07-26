-- Example Lua widget for Slate
-- Place in ~/.config/slate/scripts/greeting.lua

name = "Greeting"
description = "A friendly greeting widget"
version = "0.1.0"

local json = require("json") -- provided by host

function refresh()
    local hour = os.date("*t").hour
    local greeting
    if hour < 12 then
        greeting = "Good morning!"
    elseif hour < 17 then
        greeting = "Good afternoon!"
    else
        greeting = "Good evening!"
    end

    return '{"type":"text","content":"' .. greeting .. '\\nWelcome to Slate.","scrollable":false,"wrap":true}'
end
