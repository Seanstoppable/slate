-- Kubernetes -- shows cluster objects (pods, deployments, nodes)
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
            local err_msg = result.stderr:gsub("\n", " "):sub(1, 80)
            if result.stderr:match("not found") or result.stderr:match("Unable to connect") then
                table.insert(items, {id = obj_type .. "-err", title = "\226\154\160 " .. obj_type .. ": cluster unreachable", subtitle = err_msg})
            else
                table.insert(items, {id = obj_type .. "-err", title = "\226\154\160 " .. obj_type .. ": error", subtitle = err_msg})
            end
        else
            -- Section header
            table.insert(items, {id = obj_type .. "-header", title = "\226\148\128\226\148\128 " .. obj_type:upper() .. " \226\148\128\226\148\128", subtitle = ""})

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
                        title_str = status_icon(status) .. " " .. title_str
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
                            title_str = "\226\156\147 " .. title_str
                        else
                            title_str = "\226\154\160 " .. title_str
                        end
                        subtitle_str = ready .. "/" .. desired .. " ready"
                        if ns ~= "" then subtitle_str = subtitle_str .. " [" .. ns .. "]" end
                    elseif obj_type == "nodes" then
                        local status = parts[2] or "?"
                        title_str = "\226\172\161 " .. title_str
                        subtitle_str = status
                    end

                    table.insert(items, {id = obj_type .. "-" .. count, title = title_str, subtitle = subtitle_str})
                end
            end

            if count == 0 then
                table.insert(items, {id = obj_type .. "-empty", title = "  (no " .. obj_type .. " found)", subtitle = ""})
            end
        end
    end

    if #items == 0 then
        return slate.text("kubectl not available")
    end

    return slate.list(items, {selectable = true})
end

function status_icon(status)
    if status == "Running" then return "\226\151\143"
    elseif status == "Succeeded" or status == "Completed" then return "\226\156\147"
    elseif status == "Pending" then return "\226\151\140"
    elseif status == "Failed" or status == "Error" or status == "CrashLoopBackOff" then return "\226\156\151"
    else return "?"
    end
end

function on_action(action_id, item_id)
    if item_id:match("%-header$") or item_id:match("%-err$") or item_id:match("%-empty$") then
        return nil
    end

    local obj_type = item_id:match("^(.-)%-[0-9]+$")
    if not obj_type then return nil end

    local context = nil
    local namespace = nil
    if config_json then
        context = config_json:match('"context"%s*:%s*"(.-)"')
        namespace = config_json:match('"namespace"%s*:%s*"(.-)"')
    end

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
        return slate.notify("Could not list resources")
    end

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
                    resource_ns = parts[1]
                    resource_name = parts[2]
                end
                break
            end
        end
    end

    if not resource_name then
        return slate.notify("Resource not found")
    end

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
        return slate.notify(result.stderr:sub(1, 80))
    end

    return slate.show_detail(result.stdout)
end