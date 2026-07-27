-- Brew Outdated — shows packages that need updating
-- Usage: type = "lua:scripts/brew-outdated.lua"
-- Platforms: macOS, Linux (with Homebrew/Linuxbrew)

name = "Brew Outdated"
description = "Shows Homebrew packages with available updates"
version = "0.1.0"

function refresh()
    local result = slate.exec("brew", {"outdated", "--verbose"})
    if result.exit_code == -1 then
        return '{"type":"text","content":"brew not found","scrollable":false,"wrap":true}'
    end

    local output = result.stdout
    if output == nil or output == "" then
        return '{"type":"text","content":"✓ All packages up to date","scrollable":false,"wrap":false}'
    end

    -- Parse "package (installed) < available" lines into a list
    local items = {}
    for line in output:gmatch("[^\n]+") do
        local pkg, installed, available = line:match("^(%S+)%s+%((.-)%)%s+<%s+(.+)")
        if pkg then
            table.insert(items, string.format(
                '{"id":"%s","title":"%s","subtitle":"%s → %s"}',
                pkg, pkg, installed, available
            ))
        else
            -- Simpler format: just "package (version)"
            local name_only = line:match("^(%S+)")
            if name_only then
                table.insert(items, string.format(
                    '{"id":"%s","title":"%s","subtitle":"update available"}',
                    name_only, name_only
                ))
            end
        end
    end

    if #items == 0 then
        return '{"type":"text","content":"✓ All packages up to date","scrollable":false,"wrap":false}'
    end

    return '{"type":"list","items":[' .. table.concat(items, ",") .. '],"selectable":false}'
end
