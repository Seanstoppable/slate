// Wego Weather plugin — runs the `wego` CLI and displays weather output.
// Language: AssemblyScript (compiled to WASM)

// Host function imports (Extism convention)
@external("extism:host/user", "exec_command")
declare function exec_command(offset: u64): u64;

// Extism PDK memory helpers (namespace: extism:host/env, short names)
@external("extism:host/env", "alloc")
declare function extism_alloc(size: u64): u64;

@external("extism:host/env", "length")
declare function extism_length(offset: u64): u64;

@external("extism:host/env", "load_u8")
declare function extism_load_u8(offset: u64): u8;

@external("extism:host/env", "store_u8")
declare function extism_store_u8(offset: u64, value: u8): void;

@external("extism:host/env", "input_length")
declare function extism_input_length(): u64;

@external("extism:host/env", "input_load_u8")
declare function extism_input_load_u8(offset: u64): u8;

@external("extism:host/env", "output_set")
declare function extism_output_set(offset: u64, length: u64): void;

function readInput(): string {
  const len = extism_input_length();
  let buf = "";
  for (let i: u64 = 0; i < len; i++) {
    buf += String.fromCharCode(extism_input_load_u8(i));
  }
  return buf;
}

function writeOutput(s: string): void {
  const encoded = String.UTF8.encode(s);
  const len: u64 = encoded.byteLength as u64;
  const offset = extism_alloc(len);
  const view = Uint8Array.wrap(encoded);
  for (let i: i32 = 0; i < view.length; i++) {
    extism_store_u8(offset + (i as u64), view[i]);
  }
  extism_output_set(offset, len);
}

function allocString(s: string): u64 {
  const encoded = String.UTF8.encode(s);
  const len: u64 = encoded.byteLength as u64;
  const offset = extism_alloc(len);
  const view = Uint8Array.wrap(encoded);
  for (let i: i32 = 0; i < view.length; i++) {
    extism_store_u8(offset + (i as u64), view[i]);
  }
  return offset;
}

function readMemory(offset: u64): string {
  const len = extism_length(offset);
  let buf = "";
  for (let i: u64 = 0; i < len; i++) {
    buf += String.fromCharCode(extism_load_u8(offset + i));
  }
  return buf;
}

function callExec(cmd: string, args: string[]): string {
  let argsJson = "[";
  for (let i = 0; i < args.length; i++) {
    if (i > 0) argsJson += ",";
    argsJson += '"' + args[i] + '"';
  }
  argsJson += "]";

  const request = '{"cmd":"' + cmd + '","args":' + argsJson + "}";
  const inputOffset = allocString(request);
  const resultOffset = exec_command(inputOffset);
  return readMemory(resultOffset);
}

export function metadata(): i32 {
  writeOutput(
    '{"name":"Wego Weather","description":"Weather display via wego CLI","version":"0.1.0","author":"Slate Community"}'
  );
  return 0;
}

export function refresh(): i32 {
  const input = readInput();

  // Parse days from settings (default: 0 = today only)
  let days = "0";
  if (input.includes('"days"')) {
    // Simple extraction - look for "days":"N" or "days":N
    const idx = input.indexOf('"days"');
    const colonIdx = input.indexOf(":", idx + 6);
    if (colonIdx > 0) {
      const afterColon = input.substring(colonIdx + 1).trim();
      if (afterColon.length > 0) {
        const firstChar = afterColon.charAt(0);
        if (firstChar == '"') {
          const endQuote = afterColon.indexOf('"', 1);
          if (endQuote > 0) days = afterColon.substring(1, endQuote);
        } else {
          // Numeric
          let numStr = "";
          for (let i = 0; i < afterColon.length; i++) {
            const c = afterColon.charCodeAt(i);
            if (c >= 48 && c <= 57) numStr += afterColon.charAt(i);
            else break;
          }
          if (numStr.length > 0) days = numStr;
        }
      }
    }
  }

  const result = callExec("wego", [days]);

  // Parse exec response
  let stdout = "";
  let exitCode = -1;

  if (result.includes('"stdout"')) {
    const stdoutIdx = result.indexOf('"stdout"');
    const colonIdx = result.indexOf(":", stdoutIdx + 8);
    const quoteStart = result.indexOf('"', colonIdx + 1);
    if (quoteStart > 0) {
      // Find matching end quote (handle escaped quotes)
      let i = quoteStart + 1;
      while (i < result.length) {
        if (result.charAt(i) == "\\" && i + 1 < result.length) {
          i += 2;
          continue;
        }
        if (result.charAt(i) == '"') break;
        i++;
      }
      stdout = result.substring(quoteStart + 1, i);
      // Unescape newlines
      stdout = stdout.replaceAll("\\n", "\n");
      stdout = stdout.replaceAll("\\t", "\t");
      stdout = stdout.replaceAll('\\"', '"');
    }
  }

  if (result.includes('"exit_code"')) {
    const ecIdx = result.indexOf('"exit_code"');
    const colonIdx = result.indexOf(":", ecIdx + 11);
    const afterColon = result.substring(colonIdx + 1).trim();
    let numStr = "";
    for (let i = 0; i < afterColon.length; i++) {
      const c = afterColon.charCodeAt(i);
      if (c >= 48 && c <= 57 || (c == 45 && i == 0)) numStr += afterColon.charAt(i);
      else break;
    }
    if (numStr.length > 0) exitCode = parseInt(numStr) as i32;
  }

  if (exitCode != 0 || stdout.length == 0) {
    writeOutput(
      '{"type":"text","content":"wego not available. Install: go install github.com/schachmat/wego@latest","scrollable":false,"wrap":true}'
    );
    return 0;
  }

  // Display raw wego output as scrollable text (it includes ASCII art weather)
  // Escape for JSON
  let escaped = stdout.replaceAll("\\", "\\\\");
  escaped = escaped.replaceAll('"', '\\"');
  escaped = escaped.replaceAll("\n", "\\n");
  escaped = escaped.replaceAll("\t", "\\t");

  writeOutput('{"type":"text","content":"' + escaped + '","scrollable":true,"wrap":false}');
  return 0;
}

export function on_key(): i32 {
  writeOutput("");
  return 0;
}

export function on_action(): i32 {
  writeOutput("");
  return 0;
}
