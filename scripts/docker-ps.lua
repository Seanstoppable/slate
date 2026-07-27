-- Docker PS — shows running containers
-- Usage: type = "lua:scripts/docker-ps.lua"
-- Platforms: any (requires docker CLI)

name = "Docker"
description = "Shows running Docker containers"
version = "0.1.0"

function refresh()
    local handle = io.popen('docker ps --format "{{.Names}}|{{.Image}}|{{.Status}}|{{.Ports}}" 2>/dev/null')
    if not handle then
        return '{"type":"text","content":"docker not found","scrollable":false,"wrap":true}'
    end

    local output = handle:read("*a")
    handle:close()

    if output == nil or output == "" then
        -- Maybe docker isn't running or no containers
        local check = io.popen("docker info 2>&1")
        if check then
            local info = check:read("*a")
            check:close()
            if info:match("Cannot connect") or info:match("error") then
                return '{"type":"text","content":"⚠ Docker daemon not running","scrollable":false,"wrap":true}'
            end
        end
        return '{"type":"text","content":"No running containers","scrollable":false,"wrap":false}'
    end

    local items = {}
    for line in output:gmatch("[^\n]+") do
        local name, image, status, ports = line:match("^(.-)|(.-)|(.-)|(.*)")
        if name then
            local subtitle = image
            if ports and ports ~= "" then
                subtitle = subtitle .. " • " .. ports:gsub("0%.0%.0%.0:", ":")
            end
            local color = "green"
            if status:match("Paused") then color = "yellow"
            elseif status:match("Restarting") then color = "red"
            end
            table.insert(items, string.format(
                '{"id":"%s","title":"%s","subtitle":"%s"}',
                escape(name), escape(name .. " (" .. status .. ")"), escape(subtitle)
            ))
        end
    end

    if #items == 0 then
        return '{"type":"text","content":"No running containers","scrollable":false,"wrap":false}'
    end

    return '{"type":"list","items":[' .. table.concat(items, ",") .. '],"selectable":true}'
end

function escape(s)
    return s:gsub('"', '\\"'):gsub("\n", "\\n")
end
