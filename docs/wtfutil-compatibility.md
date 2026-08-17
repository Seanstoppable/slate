# wtfutil Compatibility Matrix

Status of wtfutil module equivalents in Slate.

## Migrated (23)

Modules that have been fully implemented in Slate.

| wtfutil Module | Slate Type | Slate Name | Permissions Needed | Needs API Token | Description | Notes |
|----------------|------------|------------|--------------------|-----------------|-------------|-------|
| clocks | Plugin | `clock` | — | No | Multi-timezone clock list | Uses WASI time; host injects timezone names |
| cmdrunner | Lua | `scripts/` | exec | No | Run arbitrary commands | Any Lua script can exec commands |
| devto | Plugin | `devto` | network | No | Dev.to articles | Filter by tag, username, state |
| digitalclock | Plugin | `digitalclock` | — | No | Large ASCII art time display | Uses WASI time; supports 12/24h |
| docker | Plugin | `docker` | exec | No | Container list/status | Runs `docker ps` |
| feedreader | Plugin | `feedreader` | network | No | RSS/Atom feed reader | Hosts are derived from configured feed URLs |
| git | Plugin | `vcs` | exec | No | Git repo status | Branch, staged, modified, commits |
| github | Plugin | `github` | network | Yes | PRs, issues, repo stats | |
| hackernews | Plugin | `hackernews` | network | No | Top stories with actions | Selectable list, open in browser |
| ipaddresses | Builtin | `ipaddresses` | — | No | Local network interface IPs | |
| kubernetes | Lua | `scripts/kubernetes.lua` | exec | No | Pod status | Runs kubectl |
| lunarphase | Plugin | `lunarphase` | — | No | Moon phase calculator | Pure computation, no network |
| mercurial | Plugin | `vcs` | exec | No | Hg repo status | Set `engine = "hg"` |
| pihole | Plugin | `pihole` | network | No | Pi-hole DNS filtering stats | `apiUrl` host is allowlisted; auth optional for summary |
| power | Builtin | `power` | — | No | Battery/charge state | |
| resourceusage | Builtin | `resource_usage` | — | No | CPU, memory, swap, temp | |
| security | Builtin | `firewall` | — | No | Firewall status/rules | Renamed from `security` |
| spacex | Lua | `scripts/spacex.lua` | network | No | Next SpaceX launch info | Uses public SpaceX API |
| status | Plugin | `status-pages` | network | No | Service status pages | GitHub, Slack, etc. |
| stocks/yfinance | Plugin | `yfinance` | network | No | Stock prices via Yahoo Finance | Public chart API |
| subreddit | Plugin | `subreddit` | network | No | Reddit subreddit posts | Sort by hot/new/top; NSFW filter |
| system | Builtin | `resource_usage` | — | No | System info | Merged into resource_usage |
| textfile | Builtin | `logfile` | — | No | Display/tail any text file | |
| urlcheck | Plugin | `urlcheck` | network | No | URL health check | Configured URL hosts are allowlisted; selectable list; HEAD requests |
| weatherservices | Plugin | `weather` | network | Yes | Weather forecast | OpenWeatherMap API |

## Planned (42)

Modules that are feasible but not yet implemented.

| wtfutil Module | Permissions Needed | Needs API Token | Description | Notes |
|----------------|--------------------|-----------------|-------------|-------|
| airbrake | network | Yes | Error tracking dashboard | |
| asana | network | Yes | Task management viewer | |
| azuredevops | network | Yes | CI/CD pipeline status | |
| azurelogs | network | Yes | Azure log viewer | |
| bamboohr | network | Yes | HR time-off/directory | |
| buildkite | network | Yes | CI build status | |
| circleci | network | Yes | CI pipeline status | |
| datadog | network | Yes | Monitoring dashboard | |
| digitalocean | network | Yes | Cloud droplet status | |
| football | network | Yes | Sports scores | |
| gcal | network | Yes (OAuth) | Google Calendar events | Needs OAuth flow |
| gerrit | network | Yes | Code review status | |
| gitlab | network | Yes | GitLab MRs and issues | |
| gitlabtodo | network | Yes | GitLab todo items | |
| googleanalytics | network | Yes (OAuth) | Site analytics | Needs OAuth flow |
| grafana | network | Yes | Alert/dashboard status | |
| gspreadsheets | network | Yes (OAuth) | Google Sheets viewer | Needs OAuth flow |
| healthchecks | network | Yes | Cron job monitoring | |
| hibp | network | Yes | Breach notification check | |
| jenkins | network | Yes | CI build status | |
| jira | network | Yes | Issue tracking viewer | |
| krisinformation | network | No | Swedish crisis alerts | Public API |
| nbascore | network | No | Basketball scores | Public APIs available |
| newrelic | network | Yes | APM dashboard | |
| nextbus | network | No | Transit arrival times | Public APIs available |
| opsgenie | network | Yes | Alert management | |
| pagerduty | network | Yes | Incident management | |
| ping | raw_network | No | ICMP ping latency | |
| pivotal | network | Yes | Pivotal Tracker stories | |
| pocket | network | Yes (OAuth) | Saved articles list | Needs OAuth flow |
| rollbar | network | Yes | Error tracking dashboard | |
| steam | network | Yes | Game library/friends | |
| stocks/finnhub | network | Yes | Stock prices via Finnhub API | |
| todo | storage | No | Local todo list | Needs KV storage |
| todo_plus | storage | No | Enhanced todo with priorities | |
| transmission | network | Yes | Torrent client status | |
| twitch | network | Yes | Stream online status | |
| updown | network | Yes | Uptime monitoring | |
| uptimekuma | network | Yes | Self-hosted uptime monitor | |
| uptimerobot | network | Yes | Uptime monitoring | |
| victorops | network | Yes | Incident management | |
| zendesk | network | Yes | Support ticket viewer | |

## Not Planned (14)

Modules that are deprecated, internal, or not applicable.

| wtfutil Module | Reason |
|----------------|--------|
| bargraph | Demo/placeholder widget |
| cryptocurrency/bittrex | Bittrex exchange shut down in 2023 |
| cryptocurrency/blockfolio | Rebranded to FTX (now defunct) |
| cryptocurrency/cryptolive | Niche; community can create plugin |
| cryptocurrency/mempool | Niche; community can create plugin |
| gitter | Gitter shut down in 2023; migrated to Matrix |
| logger | wtfutil internal debug log, not a user-facing feature |
| progress | Demo/placeholder widget |
| spotify | Spotify deprecated local CLI; web API needs OAuth |
| spotifyweb | Spotify web API requires complex OAuth flow |
| travisci | Travis CI is largely deprecated |
| twitter | Twitter/X API deprecated for free tier |
| twitterstats | Twitter/X API deprecated for free tier |
| unknown | Internal wtfutil placeholder |

## Summary

| Category | Count |
|----------|-------|
| ✅ Migrated | 23 |
| 🔲 Planned | 42 |
| ➖ Not Planned | 14 |
| **Total** | 79 |

## Slate-Only Features (not in wtfutil)

| Plugin/Feature | Type | Permissions Needed | Needs API Token | Description |
|----------------|------|--------------------|-----------------| ------------|
| `brew-outdated` | Plugin | exec | No | Outdated Homebrew packages |
| `ipinfo` | Plugin | network | No | Public IP geolocation (ip-api.com) |
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
