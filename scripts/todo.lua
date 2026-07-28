-- Todo -- reads a todo.txt file and displays it
-- Usage: type = "lua:scripts/todo.lua"
-- Settings: file = "~/todo.txt"

name = "Todo"
description = "Displays tasks from a todo.txt file"
version = "0.1.0"

function refresh()
    local filepath = "~/todo.txt"
    if config_json then
        local f = config_json:match('"file"%s*:%s*"(.-)"')
        if f then filepath = f end
    end

    -- Expand ~ to home directory
    local home = slate.env("HOME") or slate.env("USERPROFILE") or ""
    filepath = filepath:gsub("^~", home)

    local content = slate.read_file(filepath)
    if not content then
        return slate.text("No todo file found at " .. filepath .. "\nCreate one to get started!")
    end

    local items = {}
    local line_num = 0
    for line in content:gmatch("[^\n]+") do
        line_num = line_num + 1
        line = line:match("^%s*(.-)%s*$")  -- trim
        if line ~= "" then
            local done = line:match("^x%s") ~= nil
            local priority = line:match("^%((%u)%)")
            local title = line
            if done then title = "\226\156\147 " .. line:sub(3) end

            local subtitle = ""
            if priority then subtitle = "Priority: " .. priority end

            table.insert(items, {
                id = tostring(line_num),
                title = title,
                subtitle = subtitle,
            })
        end
    end

    if #items == 0 then
        return slate.text("\240\159\142\137 All done! No tasks.", {wrap = false})
    end

    return slate.list(items, {selectable = true})
end