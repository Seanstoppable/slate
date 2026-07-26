// iStats plugin — displays system stats (temperature, fans, battery) via iStats CLI.
// Language: Zig (compiled to wasm32-freestanding, using Extism ABI)

// Extism kernel imports (namespace: "extism:host/env", short names as per SDK convention)
extern "extism:host/env" fn alloc(size: u64) u64;
extern "extism:host/env" fn store_u8(offset: u64, value: u8) void;
extern "extism:host/env" fn output_set(offset: u64, length: u64) void;
extern "extism:host/env" fn length(offset: u64) u64;
extern "extism:host/env" fn load_u8(offset: u64) u8;
extern "extism:host/env" fn input_length() u64;

// Custom host function
extern "extism:host/user" fn exec_command(offset: u64) u64;

fn output(s: []const u8) void {
    const len: u64 = @intCast(s.len);
    const offset = alloc(len);
    for (0..s.len) |i| {
        store_u8(offset + @as(u64, @intCast(i)), s[i]);
    }
    output_set(offset, len);
}

fn allocStr(s: []const u8) u64 {
    const len: u64 = @intCast(s.len);
    const offset = alloc(len);
    for (0..s.len) |i| {
        store_u8(offset + @as(u64, @intCast(i)), s[i]);
    }
    return offset;
}

fn readMem(offset: u64, buf: []u8) usize {
    const len = length(offset);
    const to_read: usize = @intCast(@min(len, @as(u64, @intCast(buf.len))));
    for (0..to_read) |i| {
        buf[i] = load_u8(offset + @as(u64, @intCast(i)));
    }
    return to_read;
}

export fn metadata() i32 {
    const meta =
        \\{"name":"iStats","description":"System stats via iStats (macOS)","version":"0.1.0","author":"Slate Community"}
    ;
    output(meta);
    return 0;
}

export fn refresh() i32 {
    // Call exec_command with {"cmd":"istats","args":["all"]}
    const req = "{\"cmd\":\"istats\",\"args\":[\"all\"]}";
    const req_offset = allocStr(req);
    const result_offset = exec_command(req_offset);

    // Read result into buffer
    var result_buf: [8192]u8 = undefined;
    const result_len = readMem(result_offset, &result_buf);
    const result_str = result_buf[0..result_len];

    // Find stdout in the JSON response (simple search)
    const stdout_marker = "\"stdout\":\"";
    const stdout_start = findSubstring(result_str, stdout_marker);
    if (stdout_start == null) {
        const fallback =
            \\{"type":"text","content":"Failed to run istats. Install with: gem install iStats","scrollable":false,"wrap":true}
        ;
        output(fallback);
        return 0;
    }

    const content_start = stdout_start.? + stdout_marker.len;
    // Find end quote (handle escaped quotes)
    var end_pos: usize = content_start;
    while (end_pos < result_len) {
        if (result_str[end_pos] == '\\' and end_pos + 1 < result_len) {
            end_pos += 2;
            continue;
        }
        if (result_str[end_pos] == '"') break;
        end_pos += 1;
    }

    const stdout_content = result_str[content_start..end_pos];

    // Check if stdout is empty (istats not installed)
    if (stdout_content.len == 0) {
        const not_found =
            \\{"type":"text","content":"istats not found. Install: gem install iStats","scrollable":false,"wrap":true}
        ;
        output(not_found);
        return 0;
    }

    // Build key_value pairs from iStats output
    // Each line is like "CPU temp:  45.0°C  ▁▂▃▅▆▇"
    var out_buf: [4096]u8 = undefined;
    var out_pos: usize = 0;

    // Write opening
    const header = "{\"type\":\"key_value\",\"pairs\":[";
    @memcpy(out_buf[out_pos..][0..header.len], header);
    out_pos += header.len;

    var first = true;
    var line_start: usize = 0;
    var i: usize = 0;
    while (i <= stdout_content.len) {
        const is_newline = if (i < stdout_content.len)
            (stdout_content[i] == 'n' and i > 0 and stdout_content[i - 1] == '\\')
        else
            true;

        if (is_newline or i == stdout_content.len) {
            const line_end = if (is_newline and i < stdout_content.len) i - 1 else i;
            const line = stdout_content[line_start..line_end];

            if (line.len > 0 and !startsWith(line, "---")) {
                if (findSubstring(line, ":")) |colon_pos| {
                    const key = trimSpaces(line[0..colon_pos]);
                    const value = trimSpaces(line[colon_pos + 1 ..]);
                    if (key.len > 0) {
                        if (!first) {
                            out_buf[out_pos] = ',';
                            out_pos += 1;
                        }
                        first = false;

                        const pair_start = "{\"key\":\"";
                        @memcpy(out_buf[out_pos..][0..pair_start.len], pair_start);
                        out_pos += pair_start.len;
                        @memcpy(out_buf[out_pos..][0..key.len], key);
                        out_pos += key.len;
                        const mid = "\",\"value\":\"";
                        @memcpy(out_buf[out_pos..][0..mid.len], mid);
                        out_pos += mid.len;
                        @memcpy(out_buf[out_pos..][0..value.len], value);
                        out_pos += value.len;
                        const end_pair = "\"}";
                        @memcpy(out_buf[out_pos..][0..end_pair.len], end_pair);
                        out_pos += end_pair.len;
                    }
                }
            }

            line_start = if (is_newline) i + 1 else i;
        }
        i += 1;
    }

    // Close
    const footer = "]}";
    @memcpy(out_buf[out_pos..][0..footer.len], footer);
    out_pos += footer.len;

    output(out_buf[0..out_pos]);
    return 0;
}

export fn on_key() i32 {
    output("");
    return 0;
}

export fn on_action() i32 {
    output("");
    return 0;
}

fn findSubstring(haystack: []const u8, needle: []const u8) ?usize {
    if (needle.len > haystack.len) return null;
    var j: usize = 0;
    while (j <= haystack.len - needle.len) {
        if (eql(haystack[j .. j + needle.len], needle)) return j;
        j += 1;
    }
    return null;
}

fn eql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    for (0..a.len) |k| {
        if (a[k] != b[k]) return false;
    }
    return true;
}

fn startsWith(s: []const u8, prefix: []const u8) bool {
    if (s.len < prefix.len) return false;
    return eql(s[0..prefix.len], prefix);
}

fn trimSpaces(s: []const u8) []const u8 {
    var start: usize = 0;
    while (start < s.len and (s[start] == ' ' or s[start] == '\\')) {
        if (s[start] == '\\') {
            if (start + 1 < s.len and s[start + 1] == 't') {
                start += 2;
                continue;
            }
        }
        start += 1;
    }
    var end: usize = s.len;
    while (end > start and (s[end - 1] == ' ' or s[end - 1] == '\\')) {
        if (end >= 2 and s[end - 2] == '\\' and s[end - 1] == 't') {
            end -= 2;
            continue;
        }
        end -= 1;
    }
    return s[start..end];
}
