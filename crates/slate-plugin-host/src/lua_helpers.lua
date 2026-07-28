-- Slate Lua helpers — injected into every Lua script environment.
-- Provides content builders so scripts don't construct raw JSON.

-- JSON string escaping
function slate.escape(s)
    if s == nil then return "" end
    s = tostring(s)
    s = s:gsub('\\', '\\\\')
    s = s:gsub('"', '\\"')
    s = s:gsub('\n', '\\n')
    s = s:gsub('\r', '')
    s = s:gsub('\t', '\\t')
    return s
end

-- Encode a Lua value to JSON (primitives, arrays, objects)
function slate.json_encode(val)
    if val == nil then
        return "null"
    end
    local t = type(val)
    if t == "boolean" then
        return val and "true" or "false"
    elseif t == "number" then
        return tostring(val)
    elseif t == "string" then
        return '"' .. slate.escape(val) .. '"'
    elseif t == "table" then
        -- Detect array vs object: array has sequential integer keys starting at 1
        local is_array = true
        local max_i = 0
        for k, _ in pairs(val) do
            if type(k) ~= "number" or k ~= math.floor(k) or k < 1 then
                is_array = false
                break
            end
            if k > max_i then max_i = k end
        end
        if is_array and max_i == #val then
            local parts = {}
            for i = 1, #val do
                table.insert(parts, slate.json_encode(val[i]))
            end
            return "[" .. table.concat(parts, ",") .. "]"
        else
            local parts = {}
            for k, v in pairs(val) do
                table.insert(parts, '"' .. slate.escape(tostring(k)) .. '":' .. slate.json_encode(v))
            end
            return "{" .. table.concat(parts, ",") .. "}"
        end
    end
    return "null"
end

--- Content builders ---

-- slate.text(content, opts?) -> JSON string
-- opts: { scrollable = bool, wrap = bool }
function slate.text(content, opts)
    opts = opts or {}
    local scrollable = opts.scrollable or false
    local wrap = opts.wrap
    if wrap == nil then wrap = true end
    return slate.json_encode({
        type = "text",
        content = content,
        scrollable = scrollable,
        wrap = wrap,
    })
end

-- slate.key_value(pairs) -> JSON string
-- pairs: array of { key, value } or { key, value, color }
-- value can be a string or { text = "...", color = "..." }
function slate.key_value(pairs)
    local encoded = {}
    for _, p in ipairs(pairs) do
        local key = p[1] or p.key or ""
        local val = p[2] or p.value or ""
        if type(val) == "table" then
            -- { text = "...", color = "..." }
            local cell = '{"text":"' .. slate.escape(val.text or "") .. '"'
            if val.color then
                cell = cell .. ',"style":{"fg":"' .. slate.escape(val.color) .. '"}'
            end
            cell = cell .. '}'
            table.insert(encoded, '["' .. slate.escape(key) .. '",' .. cell .. ']')
        else
            table.insert(encoded, '["' .. slate.escape(key) .. '",{"text":"' .. slate.escape(val) .. '"}]')
        end
    end
    return '{"type":"key_value","pairs":[' .. table.concat(encoded, ",") .. ']}'
end

-- slate.list(items, opts?) -> JSON string
-- items: array of { id, title, subtitle? }
-- opts: { selectable = bool, actions = array }
function slate.list(items, opts)
    opts = opts or {}
    local selectable = opts.selectable
    if selectable == nil then selectable = false end
    local encoded = {}
    for _, item in ipairs(items) do
        local id = item.id or item[1] or ""
        local title = item.title or item[2] or ""
        local subtitle = item.subtitle or item[3] or ""
        table.insert(encoded, '{"id":"' .. slate.escape(id) .. '","title":"' .. slate.escape(title) .. '","subtitle":"' .. slate.escape(subtitle) .. '"}')
    end
    local json = '{"type":"list","items":[' .. table.concat(encoded, ",") .. '],"selectable":' .. (selectable and "true" or "false")
    if opts.actions then
        local act_parts = {}
        for _, a in ipairs(opts.actions) do
            table.insert(act_parts, '{"id":"' .. slate.escape(a.id or "") .. '","label":"' .. slate.escape(a.label or "") .. '"}')
        end
        json = json .. ',"actions":[' .. table.concat(act_parts, ",") .. ']'
    end
    json = json .. '}'
    return json
end

-- slate.table(headers, rows, opts?) -> JSON string
-- rows: array of arrays (each cell is a string or { text, color })
-- opts: { selectable = bool }
function slate.table(headers, rows, opts)
    opts = opts or {}
    local selectable = opts.selectable or false
    local row_parts = {}
    for _, row in ipairs(rows) do
        local cells = {}
        for _, cell in ipairs(row) do
            if type(cell) == "table" then
                table.insert(cells, '{"text":"' .. slate.escape(cell.text or "") .. '","color":"' .. slate.escape(cell.color or "white") .. '"}')
            else
                table.insert(cells, '"' .. slate.escape(cell) .. '"')
            end
        end
        table.insert(row_parts, "[" .. table.concat(cells, ",") .. "]")
    end
    local hdr_parts = {}
    for _, h in ipairs(headers) do
        table.insert(hdr_parts, '"' .. slate.escape(h) .. '"')
    end
    return '{"type":"table","headers":[' .. table.concat(hdr_parts, ",") .. '],"rows":[' .. table.concat(row_parts, ",") .. '],"selectable":' .. (selectable and "true" or "false") .. '}'
end

--- Action helpers ---

-- slate.notify(message) -> JSON action string
function slate.notify(message)
    return '{"notify":"' .. slate.escape(message) .. '"}'
end

-- slate.open_url(url) -> JSON action string
function slate.open_url(url)
    return '{"open_url":"' .. slate.escape(url) .. '"}'
end

-- slate.show_detail(text) -> JSON action string
function slate.show_detail(text)
    return '{"show_detail":"' .. slate.escape(text) .. '"}'
end
