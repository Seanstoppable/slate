# wtfutil Compatibility Matrix

Status of wtfutil module equivalents in Slate.

## Legend

| Status | Meaning |
|--------|---------|
| ✅ Builtin | Implemented as a native Rust builtin |
| ✅ Plugin | Implemented as a WASM plugin |
| ✅ Lua | Implemented as a Lua script |
| 🔲 Planned | Not yet implemented but feasible |
| ➖ N/A | Not applicable or deprecated |

## Modules

| wtfutil Module | Slate Status | Slate Name | Permissions Needed | Needs API Token | Description | Notes |
|----------------|--------------|------------|--------------------|-----------------|-------------|-------|
| airbrake | 🔲 Planned | — | network | Yes | Error tracking dashboard | |
| asana | 🔲 Planned | — | network | Yes | Task management viewer | |
| azuredevops | 🔲 Planned | — | network | Yes | CI/CD pipeline status | |
| azurelogs | 🔲 Planned | — | network | Yes | Azure log viewer | |
| bamboohr | 🔲 Planned | — | network | Yes | HR time-off/directory | |
| bargraph | ➖ N/A | — | — | — | Demo/placeholder widget | |
| buildkite | 🔲 Planned | — | network | Yes | CI build status | |
| circleci | 🔲 Planned | — | network | Yes | CI pipeline status | |
| clocks | ✅ Plugin | `clock` | — | No | Multi-timezone clock list | Uses WASI time; host injects timezone names |
| cmdrunner | ✅ Lua | `scripts/` | exec | No | Run arbitrary commands | Any Lua script can exec commands |
| cryptocurrency | 🔲 Planned | — | network | No | Crypto price ticker | Public APIs available |
| datadog | 🔲 Planned | — | network | Yes | Monitoring dashboard | |
| devto | ✅ Plugin | `devto` | network | No | Dev.to articles | Filter by tag, username, state |
| digitalclock | ✅ Plugin | `digitalclock` | — | No | Large ASCII art time display | Uses WASI time; supports 12/24h |
| digitalocean | 🔲 Planned | — | network | Yes | Cloud droplet status | |
| docker | ✅ Plugin | `docker` | exec | No | Container list/status | Runs `docker ps` |
| feedreader | ✅ Plugin | `feedreader` | network | No | RSS/Atom feed reader | Supports any feed URL |
| football | 🔲 Planned | — | network | Yes | Sports scores | |
| gcal | 🔲 Planned | — | network | Yes (OAuth) | Google Calendar events | Needs OAuth flow |
| gerrit | 🔲 Planned | — | network | Yes | Code review status | |
| git | ✅ Plugin | `vcs` | exec | No | Git repo status | Branch, staged, modified, commits |
| github | ✅ Plugin | `github` | network | Yes | PRs, issues, repo stats | |
| gitlab | 🔲 Planned | — | network | Yes | GitLab MRs and issues | |
| gitlabtodo | 🔲 Planned | — | network | Yes | GitLab todo items | |
| gitter | ➖ N/A | — | — | — | Chat client | Gitter is deprecated |
| googleanalytics | 🔲 Planned | — | network | Yes (OAuth) | Site analytics | Needs OAuth flow |
| grafana | 🔲 Planned | — | network | Yes | Alert/dashboard status | |
| gspreadsheets | 🔲 Planned | — | network | Yes (OAuth) | Google Sheets viewer | Needs OAuth flow |
| hackernews | ✅ Plugin | `hackernews` | network | No | Top stories with actions | Selectable list, open in browser |
| healthchecks | 🔲 Planned | — | network | Yes | Cron job monitoring | |
| hibp | 🔲 Planned | — | network | Yes | Breach notification check | |
| ipaddresses | ✅ Builtin | `ipaddresses` | — | No | Local network interface IPs | |
| jenkins | 🔲 Planned | — | network | Yes | CI build status | |
| jira | 🔲 Planned | — | network | Yes | Issue tracking viewer | |
| krisinformation | 🔲 Planned | — | network | No | Swedish crisis alerts | Public API |
| kubernetes | ✅ Lua | `scripts/kubernetes.lua` | exec | No | Pod status | Runs kubectl |
| logger | ➖ N/A | — | — | — | wtfutil internal debug log | Not a user-facing feature |
| lunarphase | ✅ Plugin | `lunarphase` | — | No | Moon phase calculator | Pure computation, no network |
| mercurial | ✅ Plugin | `vcs` | exec | No | Hg repo status | Set `engine = "hg"` |
| nbascore | 🔲 Planned | — | network | No | Basketball scores | Public APIs available |
| newrelic | 🔲 Planned | — | network | Yes | APM dashboard | |
| nextbus | 🔲 Planned | — | network | No | Transit arrival times | Public APIs available |
| opsgenie | 🔲 Planned | — | network | Yes | Alert management | |
| pagerduty | 🔲 Planned | — | network | Yes | Incident management | |
| pihole | ✅ Plugin | `pihole` | network | No | Pi-hole DNS filtering stats | Configurable `apiUrl`; auth optional for summary |
| ping | 🔲 Planned | — | raw_network | No | ICMP ping latency | |
| pivotal | 🔲 Planned | — | network | Yes | Pivotal Tracker stories | |
| pocket | 🔲 Planned | — | network | Yes (OAuth) | Saved articles list | Needs OAuth flow |
| power | ✅ Builtin | `power` | — | No | Battery/charge state | |
| progress | ➖ N/A | — | — | — | Demo/placeholder widget | |
| resourceusage | ✅ Builtin | `resource_usage` | — | No | CPU, memory, swap, temp | |
| rollbar | 🔲 Planned | — | network | Yes | Error tracking dashboard | |
| security | ✅ Builtin | `firewall` | — | No | Firewall status/rules | Renamed from `security` |
| spacex | ✅ Lua | `scripts/spacex.lua` | network | No | Next SpaceX launch info | Uses public SpaceX API |
| spotify | 🔲 Planned | — | exec | No | Now playing (local client) | Via `spotify` CLI |
| spotifyweb | 🔲 Planned | — | network | Yes (OAuth) | Spotify web playback | Needs OAuth flow |
| status | ✅ Plugin | `status-pages` | network | No | Service status pages | GitHub, Slack, etc. |
| steam | 🔲 Planned | — | network | Yes | Game library/friends | |
| stocks | 🔲 Planned | — | network | Yes | Stock price ticker | Most APIs require keys |
| subreddit | ✅ Plugin | `subreddit` | network | No | Reddit subreddit posts | Sort by hot/new/top; NSFW filter |
| system | ✅ Builtin | `resource_usage` | — | No | System info | Merged into resource_usage |
| textfile | ✅ Builtin | `logfile` | — | No | Display/tail any text file | |
| todo | 🔲 Planned | — | storage | No | Local todo list | Needs KV storage |
| todo_plus | 🔲 Planned | — | storage | No | Enhanced todo with priorities | |
| transmission | 🔲 Planned | — | network | Yes | Torrent client status | |
| travisci | ➖ N/A | — | — | — | CI status | Travis CI is largely deprecated |
| twitch | 🔲 Planned | — | network | Yes | Stream online status | |
| twitter | ➖ N/A | — | — | — | Tweet feed | Twitter/X API deprecated for free tier |
| twitterstats | ➖ N/A | — | — | — | Account stats | Twitter/X API deprecated for free tier |
| unknown | ➖ N/A | — | — | — | Internal wtfutil placeholder | |
| updown | 🔲 Planned | — | network | Yes | Uptime monitoring | |
| uptimekuma | 🔲 Planned | — | network | Yes | Self-hosted uptime monitor | |
| uptimerobot | 🔲 Planned | — | network | Yes | Uptime monitoring | |
| urlcheck | 🔲 Planned | — | network | No | URL health check | Simple HTTP HEAD checks |
| victorops | 🔲 Planned | — | network | Yes | Incident management | |
| weatherservices | ✅ Plugin | `weather` | network | Yes | Weather forecast | OpenWeatherMap API |
| zendesk | 🔲 Planned | — | network | Yes | Support ticket viewer | |

## Summary

| Category | Count |
|----------|-------|
| ✅ Implemented | 22 |
| 🔲 Planned | 43 |
| ➖ N/A | 8 |
| **Total** | 73 |

## Slate-Only Features (not in wtfutil)

| Plugin/Feature | Type | Permissions Needed | Needs API Token | Description |
|----------------|------|--------------------|-----------------| ------------|
| `ipinfo` | Plugin | network | No | Public IP geolocation (ip-api.com) |
| `brew-outdated` | Plugin | exec | No | Outdated Homebrew packages |
| `istats` | Plugin | exec | No | macOS hardware temps via iStats |
| `wego` | Plugin | exec | No | Weather via wego CLI |

## Migration Notes

When migrating from a wtfutil `config.yml`, the following mappings require special attention:

| wtfutil Module | Slate Equivalent | Migration Notes |
|----------------|------------------|-----------------|
| `logger` | `builtin:logfile` | Set `filePath = "~/.config/wtf/log.txt"` (the wtfutil log path) |
| `textfile` | `builtin:logfile` | Map `filePaths[0]` → `filePath` setting |
| `git` | `wasm:vcs` | Map `repositories[0]` → `repo_path` setting |
| `mercurial` | `wasm:vcs` | Set `engine = "hg"`, map repo path |
| `clocks` | `wasm:clock` | Map `locations` object (label → timezone) |
| `resourceusage` + `system` | `builtin:resource_usage` | Merged into one widget |
| `security` | `builtin:firewall` | Renamed; same functionality |
| `weatherservices` | `wasm:weather` | Map `apiKey` → `api_key` secret |
| `cmdrunner` | Lua script | Convert `cmd` + `args` to a `slate.exec()` Lua script |
