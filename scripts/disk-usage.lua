-- Disk Usage — shows filesystem usage per mount point
-- Usage: type = "lua:scripts/disk-usage.lua"
-- Platforms: macOS, Linux, Windows (via wmic fallback)

name = "Disk Usage"
description = "Shows disk space usage for mounted filesystems"
version = "0.1.0"

function refresh()
    local pairs = {}

    -- Try Unix df first
    local handle = io.popen("df -h 2>/dev/null")
    if handle then
        local output = handle:read("*a")
        handle:close()

        if output and output ~= "" then
            local first = true
            for line in output:gmatch("[^\n]+") do
                if first then
                    first = false  -- skip header
                else
                    local fs, size, used, avail, pct, mount =
                        line:match("^(%S+)%s+(%S+)%s+(%S+)%s+(%S+)%s+(%S+)%s+(.+)")
                    if mount and not mount:match("^/private/var/") and not mount:match("^/snap/") then
                        local bar = make_bar(pct)
                        table.insert(pairs, string.format(
                            '["%s",{"text":"%s  %s / %s","color":"%s"}]',
                            mount, bar, used, size, bar_color(pct)
                        ))
                    end
                end
            end
        end
    end

    -- Windows fallback
    if #pairs == 0 then
        local wh = io.popen('wmic logicaldisk get name,size,freespace /format:csv 2>nul')
        if wh then
            local out = wh:read("*a")
            wh:close()
            for line in out:gmatch("[^\n]+") do
                local node, free, name, total = line:match("^(%S-),(%d+),(%S-),(%d+)")
                if name and total then
                    local t = tonumber(total)
                    local f = tonumber(free)
                    if t and t > 0 then
                        local pct_num = math.floor((t - f) / t * 100)
                        local pct = tostring(pct_num) .. "%"
                        local bar = make_bar(pct)
                        table.insert(pairs, string.format(
                            '["%s",{"text":"%s  %s%%","color":"%s"}]',
                            name, bar, pct_num, bar_color(pct)
                        ))
                    end
                end
            end
        end
    end

    if #pairs == 0 then
        return '{"type":"text","content":"No disk info available","scrollable":false,"wrap":true}'
    end

    return '{"type":"key_value","pairs":[' .. table.concat(pairs, ",") .. ']}'
end

function make_bar(pct_str)
    local num = tonumber(pct_str:match("(%d+)")) or 0
    local filled = math.floor(num / 10)
    return string.rep("█", filled) .. string.rep("░", 10 - filled)
end

function bar_color(pct_str)
    local num = tonumber(pct_str:match("(%d+)")) or 0
    if num >= 90 then return "red"
    elseif num >= 70 then return "yellow"
    else return "green"
    end
end
