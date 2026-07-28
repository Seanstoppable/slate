-- Brew Outdated -- shows packages that need updating
-- Usage: type = "lua:scripts/brew-outdated.lua"
-- Platforms: macOS, Linux (with Homebrew/Linuxbrew)

name = "Brew Outdated"
description = "Shows Homebrew packages with available updates"
version = "0.1.0"

function refresh()
    local result = slate.exec("brew", {"outdated", "--verbose"})
    if result.exit_code == -1 then
        return slate.text("brew not found")
    end

    local output = result.stdout
    if output == nil or output == "" then
        return slate.text("\226\156\147 All packages up to date", {wrap = false})
    end

    -- Parse "package (installed) < available" lines into a list
    local items = {}
    for line in output:gmatch("[^\n]+") do
        local pkg, installed, available = line:match("^(%S+)%s+%((.-)%)%s+<%s+(.+)")
        if pkg then
            table.insert(items, {id = pkg, title = pkg, subtitle = installed .. " \226\134\146 " .. available})
        else
            local name_only = line:match("^(%S+)")
            if name_only then
                table.insert(items, {id = name_only, title = name_only, subtitle = "update available"})
            end
        end
    end

    if #items == 0 then
        return slate.text("\226\156\147 All packages up to date", {wrap = false})
    end

    return slate.list(items)
end