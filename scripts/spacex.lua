-- SpaceX next launch widget for Slate
-- Uses the public SpaceX API (no auth required)
-- Place in ~/.config/slate/scripts/spacex.lua

name = "SpaceX"
description = "Next SpaceX launch information"
version = "0.1.0"

function refresh()
    local result = slate.http("https://api.spacexdata.com/v5/launches/next")
    if not result or result == "" then
        return slate.text("Failed to fetch launch data")
    end

    local launch = slate.json_decode and slate.json_decode(result) or nil
    if not launch or not launch.name then
        return slate.text("No upcoming launch found")
    end

    local pairs = {}
    table.insert(pairs, {"Mission", launch.name or "Unknown"})
    table.insert(pairs, {"Flight #", tostring(launch.flight_number or "?")})

    if launch.date_utc then
        table.insert(pairs, {"Date (UTC)", launch.date_utc:sub(1, 10)})
    end

    if launch.rocket then
        table.insert(pairs, {"Rocket", launch.rocket})
    end

    if launch.launchpad then
        table.insert(pairs, {"Launchpad", launch.launchpad})
    end

    if launch.details then
        table.insert(pairs, {"Details", launch.details})
    end

    return slate.key_value(pairs)
end