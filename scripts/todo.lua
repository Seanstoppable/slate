-- Todo — reads a todo.txt file and displays it
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
    local home = os.getenv("HOME") or os.getenv("USERPROFILE") or ""
    filepath = filepath:gsub("^~", home)

    local file = io.open(filepath, "r")
    if not file then
        return string.format(
            '{"type":"text","content":"No todo file found at %s\\nCreate one to get started!","scrollable":false,"wrap":true}',
            escape(filepath)
        )
    end

    local items = {}
    local line_num = 0
    for line in file:lines() do
        line_num = line_num + 1
        line = line:match("^%s*(.-)%s*$")  -- trim
        if line ~= "" then
            local done = line:match("^x%s") ~= nil
            local priority = line:match("^%((%u)%)")
            local title = line
            if done then title = "✓ " .. line:sub(3) end

            local subtitle = ""
            if priority then subtitle = "Priority: " .. priority end

            table.insert(items, string.format(
                '{"id":"%d","title":"%s","subtitle":"%s"}',
                line_num, escape(title), escape(subtitle)
            ))
        end
    end
    file:close()

    if #items == 0 then
        return '{"type":"text","content":"🎉 All done! No tasks.","scrollable":false,"wrap":false}'
    end

    return '{"type":"list","items":[' .. table.concat(items, ",") .. '],"selectable":true}'
end

function escape(s)
    return s:gsub('\\', '\\\\'):gsub('"', '\\"'):gsub("\n", "\\n")
end
