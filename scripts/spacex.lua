-- SpaceX next launch widget for Slate
-- Uses the public SpaceX API (no auth required)
-- Place in ~/.config/slate/scripts/spacex.lua

name = "SpaceX"
description = "Next SpaceX launch information"
version = "0.1.0"

function refresh()
    local result = slate.http("https://api.spacexdata.com/v5/launches/next")
    if not result or result == "" then
        return '{"type":"text","content":"Failed to fetch launch data","scrollable":false,"wrap":true}'
    end

    local launch = json_decode(result)
    if not launch or not launch.name then
        return '{"type":"text","content":"No upcoming launch found","scrollable":false,"wrap":true}'
    end

    local pairs = {}
    table.insert(pairs, {key = "Mission", value = launch.name or "Unknown"})
    table.insert(pairs, {key = "Flight #", value = tostring(launch.flight_number or "?")})

    if launch.date_utc then
        table.insert(pairs, {key = "Date (UTC)", value = launch.date_utc:sub(1, 10)})
    end

    if launch.rocket then
        table.insert(pairs, {key = "Rocket", value = launch.rocket})
    end

    if launch.launchpad then
        table.insert(pairs, {key = "Launchpad", value = launch.launchpad})
    end

    if launch.details then
        table.insert(pairs, {key = "Details", value = launch.details})
    end

    return json_encode({type = "key_value", pairs = pairs})
end

-- Minimal JSON helpers (Lua scripts get these from the slate runtime)
function json_decode(str)
    -- The slate runtime provides slate.json_decode
    if slate.json_decode then
        return slate.json_decode(str)
    end
    return nil
end

function json_encode(tbl)
    -- The slate runtime provides slate.json_encode
    if slate.json_encode then
        return slate.json_encode(tbl)
    end
    -- Fallback: manual key_value encoding
    if tbl.type == "key_value" then
        local parts = {}
        for _, p in ipairs(tbl.pairs) do
            local k = p.key:gsub('"', '\\"')
            local v = p.value:gsub('"', '\\"')
            table.insert(parts, '{"key":"' .. k .. '","value":"' .. v .. '"}')
        end
        return '{"type":"key_value","pairs":[' .. table.concat(parts, ",") .. ']}'
    end
    return '{}'
end
