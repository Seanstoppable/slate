-- Docker PS — shows running containers
-- Usage: type = "lua:scripts/docker-ps.lua"
-- Platforms: any (requires docker CLI)

name = "Docker"
description = "Shows running Docker containers"
version = "0.1.0"

function refresh()
    local result = slate.exec("docker", {"ps", "--format", "{{.Names}}|{{.Image}}|{{.Status}}|{{.Ports}}"})
    if result.exit_code == -1 then
        return '{"type":"text","content":"docker not found","scrollable":false,"wrap":true}'
    end

    local output = result.stdout
    if output == nil or output == "" then
        -- Maybe docker isn't running or no containers
        if result.stderr:match("Cannot connect") or result.stderr:match("error") then
            return '{"type":"text","content":"⚠ Docker daemon not running","scrollable":false,"wrap":true}'
        end
        return '{"type":"text","content":"No running containers","scrollable":false,"wrap":false}'
    end

    local items = {}
    for line in output:gmatch("[^\n]+") do
        local cname, image, status, ports = line:match("^(.-)|(.-)|(.-)|(.*)")
        if cname then
            local subtitle = image
            if ports and ports ~= "" then
                subtitle = subtitle .. " • " .. ports:gsub("0%.0%.0%.0:", ":")
            end
            table.insert(items, string.format(
                '{"id":"%s","title":"%s","subtitle":"%s"}',
                escape(cname), escape(cname .. " (" .. status .. ")"), escape(subtitle)
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
