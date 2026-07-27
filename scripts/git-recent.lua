-- Git Recent — shows recent commits in the current repo
-- Usage: type = "lua:scripts/git-recent.lua"
-- Settings: count = 10, path = "." (optional repo path)

name = "Git Recent"
description = "Shows recent git commits in a repository"
version = "0.1.0"

function refresh()
    -- Read config
    local count = 8
    local path = "."
    if config_json then
        local c = config_json
        local n = c:match('"count"%s*:%s*(%d+)')
        if n then count = tonumber(n) end
        local p = c:match('"path"%s*:%s*"(.-)"')
        if p then path = p end
    end

    local cmd = string.format(
        'git -C "%s" log --oneline --no-decorate -n %d --format="%%h|%%s|%%ar|%%an" 2>/dev/null',
        path, count
    )
    local handle = io.popen(cmd)
    if not handle then
        return '{"type":"text","content":"git not available","scrollable":false,"wrap":true}'
    end

    local output = handle:read("*a")
    handle:close()

    if output == nil or output == "" then
        return '{"type":"text","content":"Not a git repository","scrollable":false,"wrap":true}'
    end

    local items = {}
    for line in output:gmatch("[^\n]+") do
        local hash, subject, time_ago, author = line:match("^(.-)|(.-)|(.-)|(.*)")
        if hash then
            table.insert(items, string.format(
                '{"id":"%s","title":"%s","subtitle":"%s • %s • %s"}',
                escape(hash), escape(subject), escape(hash), escape(author), escape(time_ago)
            ))
        end
    end

    if #items == 0 then
        return '{"type":"text","content":"No commits found","scrollable":false,"wrap":false}'
    end

    return '{"type":"list","items":[' .. table.concat(items, ",") .. '],"selectable":true}'
end

function escape(s)
    return s:gsub('\\', '\\\\'):gsub('"', '\\"'):gsub("\n", "\\n")
end
