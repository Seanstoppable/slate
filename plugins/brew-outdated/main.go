// Brew Outdated plugin — shows outdated Homebrew packages.
// Language: Go (compiled via TinyGo to wasm32-wasip1)
package main

import (
	"encoding/json"
	"strings"

	"github.com/extism/go-pdk"
)

// ExecRequest is sent to the exec_command host function.
type ExecRequest struct {
	Cmd  string   `json:"cmd"`
	Args []string `json:"args"`
}

// ExecResponse is returned from the exec_command host function.
type ExecResponse struct {
	Stdout   string `json:"stdout"`
	Stderr   string `json:"stderr"`
	ExitCode int    `json:"exit_code"`
}

// execCommand calls the host-provided exec_command function.
//
//go:wasmimport extism:host/user exec_command
func _exec_command(offset uint64) uint64

func execCommand(cmd string, args []string) (*ExecResponse, error) {
	req := ExecRequest{Cmd: cmd, Args: args}
	reqBytes, _ := json.Marshal(req)

	mem := pdk.AllocateBytes(reqBytes)
	resultOffset := _exec_command(mem.Offset())

	resultMem := pdk.FindMemory(resultOffset)
	resultBytes := make([]byte, resultMem.Length())
	resultMem.Load(resultBytes)

	var resp ExecResponse
	if err := json.Unmarshal(resultBytes, &resp); err != nil {
		return nil, err
	}
	return &resp, nil
}

//export metadata
func metadata() int32 {
	meta := map[string]string{
		"name":        "Brew Outdated",
		"description": "Shows outdated Homebrew packages",
		"version":     "0.1.0",
		"author":      "Slate Community",
	}
	out, _ := json.Marshal(meta)
	pdk.OutputString(string(out))
	return 0
}

//export refresh
func refresh() int32 {
	// Run `brew outdated`
	result, err := execCommand("brew", []string{"outdated"})
	if err != nil {
		output := map[string]interface{}{
			"type":       "text",
			"content":    "Failed to execute brew: " + err.Error(),
			"scrollable": false,
			"wrap":       true,
		}
		out, _ := json.Marshal(output)
		pdk.OutputString(string(out))
		return 0
	}

	if result.ExitCode != 0 && result.Stderr != "" {
		output := map[string]interface{}{
			"type":       "text",
			"content":    "brew error: " + result.Stderr,
			"scrollable": false,
			"wrap":       true,
		}
		out, _ := json.Marshal(output)
		pdk.OutputString(string(out))
		return 0
	}

	lines := strings.Split(strings.TrimSpace(result.Stdout), "\n")
	if len(lines) == 0 || (len(lines) == 1 && lines[0] == "") {
		output := map[string]interface{}{
			"type":       "text",
			"content":    "✓ All Homebrew packages are up to date",
			"scrollable": false,
			"wrap":       true,
		}
		out, _ := json.Marshal(output)
		pdk.OutputString(string(out))
		return 0
	}

	// Build list items from outdated packages
	items := make([]map[string]interface{}, 0, len(lines))
	for i, line := range lines {
		parts := strings.Fields(line)
		if len(parts) == 0 {
			continue
		}
		name := parts[0]
		version := ""
		if len(parts) > 1 {
			version = strings.Join(parts[1:], " ")
		}
		items = append(items, map[string]interface{}{
			"id":       i,
			"title":    name,
			"subtitle": version,
		})
	}

	output := map[string]interface{}{
		"type":       "list",
		"items":      items,
		"selectable": true,
	}
	out, _ := json.Marshal(output)
	pdk.OutputString(string(out))
	return 0
}

//export on_key
func onKey() int32 {
	pdk.OutputString("")
	return 0
}

//export on_action
func onAction() int32 {
	pdk.OutputString("")
	return 0
}

func main() {}
