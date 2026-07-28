-- Docker PS -- shows running containers
-- Usage: type = "lua:scripts/docker-ps.lua"
-- Platforms: any (requires docker CLI)

name = "Docker"
description = "Shows running Docker containers"
version = "0.1.0"

function refresh()
    local result = slate.exec("docker", {"ps", "--format", "{{.Names}}|{{.Image}}|{{.Status}}|{{.Ports}}"})
    if result.exit_code == -1 then
        return slate.text("docker not found")
    end

    local output = result.stdout
    if output == nil or output == "" then
        if result.stderr:match("Cannot connect") or result.stderr:match("error") then
            return slate.text("\226\154\160 Docker daemon not running")
        end
        return slate.text("No running containers", {wrap = false})
    end

    local items = {}
    for line in output:gmatch("[^\n]+") do
        local cname, image, status, ports = line:match("^(.-)|(.-)|(.-)|(.*)")
        if cname then
            local subtitle = image
            if ports and ports ~= "" then
                subtitle = subtitle .. " \226\128\162 " .. ports:gsub("0%.0%.0%.0:", ":")
            end
            table.insert(items, {
                id = cname,
                title = cname .. " (" .. status .. ")",
                subtitle = subtitle,
            })
        end
    end

    if #items == 0 then
        return slate.text("No running containers", {wrap = false})
    end

    return slate.list(items, {selectable = true})
end