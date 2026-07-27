-- Kubernetes — shows cluster objects (pods, deployments, nodes)
-- Usage: type = "lua:scripts/kubernetes.lua"
-- Settings: context, namespace, objects (array of "pods", "deployments", "nodes")
-- Platforms: any (requires kubectl CLI)

name = "Kubernetes"
description = "Displays Kubernetes cluster objects via kubectl"
version = "0.1.0"

function refresh()
    -- Parse config
    local context = nil
    local namespace = nil
    local objects = {"pods"}

    if config_json then
        context = config_json:match('"context"%s*:%s*"(.-)"')
        namespace = config_json:match('"namespace"%s*:%s*"(.-)"')
        -- Parse objects array
        local objs_str = config_json:match('"objects"%s*:%s*%[(.-)%]')
        if objs_str then
            objects = {}
            for obj in objs_str:gmatch('"(.-)"') do
                table.insert(objects, obj)
            end
        end
    end

    -- Build kubectl base args
    local base_args = {}
    if context then
        table.insert(base_args, "--context")
        table.insert(base_args, context)
    end
    if namespace then
        table.insert(base_args, "-n")
        table.insert(base_args, namespace)
    else
        table.insert(base_args, "--all-namespaces")
    end

    local items = {}

    for _, obj_type in ipairs(objects) do
        local args = {"get", obj_type, "--no-headers"}
        for _, a in ipairs(base_args) do
            table.insert(args, a)
        end
        -- Add output columns based on type
        if obj_type == "pods" then
            table.insert(args, "-o")
            table.insert(args, "custom-columns=NAME:.metadata.name,STATUS:.status.phase,RESTARTS:.status.containerStatuses[0].restartCount,AGE:.metadata.creationTimestamp,NS:.metadata.namespace")
        elseif obj_type == "deployments" then
            table.insert(args, "-o")
            table.insert(args, "custom-columns=NAME:.metadata.name,READY:.status.readyReplicas,DESIRED:.spec.replicas,NS:.metadata.namespace")
        elseif obj_type == "nodes" then
            table.insert(args, "-o")
            table.insert(args, "custom-columns=NAME:.metadata.name,STATUS:.status.conditions[-1].type,ROLES:.metadata.labels.node-role\\.kubernetes\\.io/master")
        end

        local result = slate.exec("kubectl", args)

        if result.exit_code ~= 0 then
            if result.stderr:match("not found") or result.stderr:match("Unable to connect") then
                table.insert(items, string.format(
                    '{"id":"%s-err","title":"⚠ %s: cluster unreachable","subtitle":"%s"}',
                    obj_type, obj_type, escape(result.stderr:gsub("\n", " "):sub(1, 60))
                ))
            else
                table.insert(items, string.format(
                    '{"id":"%s-err","title":"⚠ %s: error","subtitle":"%s"}',
                    obj_type, obj_type, escape(result.stderr:gsub("\n", " "):sub(1, 80))
                ))
            end
        else
            -- Add section header
            table.insert(items, string.format(
                '{"id":"%s-header","title":"── %s ──","subtitle":""}',
                obj_type, obj_type:upper()
            ))

            local count = 0
            for line in result.stdout:gmatch("[^\n]+") do
                if line:match("%S") then
                    count = count + 1
                    local parts = {}
                    for part in line:gmatch("%S+") do
                        table.insert(parts, part)
                    end

                    local title_str = parts[1] or "?"
                    local subtitle_str = ""

                    if obj_type == "pods" then
                        local status = parts[2] or "?"
                        local restarts = parts[3] or "0"
                        local ns = parts[5] or ""
                        local icon = status_icon(status)
                        title_str = icon .. " " .. title_str
                        subtitle_str = status
                        if restarts ~= "0" and restarts ~= "<none>" then
                            subtitle_str = subtitle_str .. " (restarts: " .. restarts .. ")"
                        end
                        if ns ~= "" then subtitle_str = subtitle_str .. " [" .. ns .. "]" end
                    elseif obj_type == "deployments" then
                        local ready = parts[2] or "?"
                        local desired = parts[3] or "?"
                        local ns = parts[4] or ""
                        if ready == desired then
                            title_str = "✓ " .. title_str
                        else
                            title_str = "⚠ " .. title_str
                        end
                        subtitle_str = ready .. "/" .. desired .. " ready"
                        if ns ~= "" then subtitle_str = subtitle_str .. " [" .. ns .. "]" end
                    elseif obj_type == "nodes" then
                        local status = parts[2] or "?"
                        title_str = "⬡ " .. title_str
                        subtitle_str = status
                    end

                    table.insert(items, string.format(
                        '{"id":"%s-%d","title":"%s","subtitle":"%s"}',
                        obj_type, count, escape(title_str), escape(subtitle_str)
                    ))
                end
            end

            if count == 0 then
                table.insert(items, string.format(
                    '{"id":"%s-empty","title":"  (no %s found)","subtitle":""}',
                    obj_type, obj_type
                ))
            end
        end
    end

    if #items == 0 then
        return '{"type":"text","content":"kubectl not available","scrollable":false,"wrap":true}'
    end

    return '{"type":"list","items":[' .. table.concat(items, ",") .. '],"selectable":true}'
end

function status_icon(status)
    if status == "Running" then return "●"
    elseif status == "Succeeded" or status == "Completed" then return "✓"
    elseif status == "Pending" then return "◌"
    elseif status == "Failed" or status == "Error" or status == "CrashLoopBackOff" then return "✗"
    else return "?"
    end
end

function escape(s)
    if not s then return "" end
    return s:gsub('\\', '\\\\'):gsub('"', '\\"'):gsub("\n", "\\n"):gsub("\r", "")
end

function on_action(action_id, item_id)
    -- item_id format: "{type}-{index}" or "{type}-header"/"{type}-err"/"{type}-empty"
    -- For actual items, look up the resource name and describe it
    if item_id:match("%-header$") or item_id:match("%-err$") or item_id:match("%-empty$") then
        return nil
    end

    -- Parse the object type from item_id (e.g., "pods-3" -> "pods")
    local obj_type = item_id:match("^(.-)%-[0-9]+$")
    if not obj_type then return nil end

    -- We need to find the resource name. Fetch it again (simple approach)
    local context = nil
    local namespace = nil
    if config_json then
        context = config_json:match('"context"%s*:%s*"(.-)"')
        namespace = config_json:match('"namespace"%s*:%s*"(.-)"')
    end

    -- Get the specific item by index
    local idx = tonumber(item_id:match("%-([0-9]+)$"))
    if not idx then return nil end

    local args = {"get", obj_type, "--no-headers"}
    if context then
        table.insert(args, "--context")
        table.insert(args, context)
    end
    if namespace then
        table.insert(args, "-n")
        table.insert(args, namespace)
    else
        table.insert(args, "--all-namespaces")
    end

    local list_result = slate.exec("kubectl", args)
    if list_result.exit_code ~= 0 then
        return '{"notify":"Could not list resources"}'
    end

    -- Find the Nth resource name
    local count = 0
    local resource_name = nil
    local resource_ns = nil
    for line in list_result.stdout:gmatch("[^\n]+") do
        if line:match("%S") then
            count = count + 1
            if count == idx then
                local parts = {}
                for part in line:gmatch("%S+") do
                    table.insert(parts, part)
                end
                if namespace then
                    resource_name = parts[1]
                    resource_ns = namespace
                else
                    -- all-namespaces: first col is namespace
                    resource_ns = parts[1]
                    resource_name = parts[2]
                end
                break
            end
        end
    end

    if not resource_name then
        return '{"notify":"Resource not found"}'
    end

    -- Run kubectl describe
    local describe_args = {"describe", obj_type:gsub("s$", ""), resource_name}
    if context then
        table.insert(describe_args, "--context")
        table.insert(describe_args, context)
    end
    if resource_ns then
        table.insert(describe_args, "-n")
        table.insert(describe_args, resource_ns)
    end

    local result = slate.exec("kubectl", describe_args)
    if result.exit_code ~= 0 then
        return '{"notify":"' .. escape(result.stderr:sub(1, 80)) .. '"}'
    end

    return '{"show_detail":"' .. escape(result.stdout) .. '"}'
end
