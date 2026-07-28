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

| wtfutil Module | Slate Status | Slate Name | Notes |
|----------------|--------------|------------|-------|
| airbrake | 🔲 Planned | — | Error tracking; needs network |
| asana | 🔲 Planned | — | Task management; needs network + secrets |
| azuredevops | 🔲 Planned | — | CI/CD status; needs network + secrets |
| azurelogs | 🔲 Planned | — | Log viewer; needs network + secrets |
| bamboohr | 🔲 Planned | — | HR; needs network + secrets |
| bargraph | 🔲 Planned | — | Generic bar chart widget |
| buildkite | 🔲 Planned | — | CI status; needs network + secrets |
| circleci | 🔲 Planned | — | CI status; needs network + secrets |
| clocks | ✅ Plugin | `clock` | Multi-timezone clock list (WASI time) |
| cmdrunner | ✅ Lua | `scripts/` | Any Lua script can exec commands |
| cryptocurrency | 🔲 Planned | — | Crypto prices; needs network |
| datadog | 🔲 Planned | — | Monitoring; needs network + secrets |
| devto | 🔲 Planned | — | Dev.to feed; feedreader covers this |
| digitalclock | 🔲 Planned | — | ASCII art clock (was builtin, removed) |
| digitalocean | 🔲 Planned | — | Cloud infra; needs network + secrets |
| docker | ✅ Plugin | `docker` | Container status via exec |
| feedreader | ✅ Plugin | `feedreader` | RSS/Atom feed reader |
| football | 🔲 Planned | — | Sports scores; needs network |
| gcal | 🔲 Planned | — | Google Calendar; needs OAuth |
| gerrit | 🔲 Planned | — | Code review; needs network + secrets |
| git | ✅ Plugin | `vcs` | Git repo status via exec |
| github | ✅ Plugin | `github` | PRs, issues, repo stats |
| gitlab | 🔲 Planned | — | Similar to GitHub; needs network + secrets |
| gitlabtodo | 🔲 Planned | — | GitLab todos; needs network + secrets |
| gitter | ➖ N/A | — | Gitter is deprecated |
| googleanalytics | 🔲 Planned | — | Analytics; needs OAuth |
| grafana | 🔲 Planned | — | Dashboard alerts; needs network + secrets |
| gspreadsheets | 🔲 Planned | — | Google Sheets; needs OAuth |
| hackernews | ✅ Plugin | `hackernews` | Top stories with actions |
| healthchecks | 🔲 Planned | — | Cron monitoring; needs network |
| hibp | 🔲 Planned | — | Have I Been Pwned; needs network |
| ipaddresses | ✅ Builtin | `ipaddresses` | Local network interface IPs |
| jenkins | 🔲 Planned | — | CI status; needs network + secrets |
| jira | 🔲 Planned | — | Issue tracking; needs network + secrets |
| krisinformation | 🔲 Planned | — | Swedish crisis info; needs network |
| kubernetes | ✅ Lua | `scripts/kubernetes.lua` | Pod status via kubectl |
| logger | ✅ Builtin | `logfile` | Tail a log file |
| lunarphase | ✅ Plugin | `lunarphase` | Moon phase calculator |
| mercurial | ✅ Plugin | `vcs` | Hg repo status (engine: hg) |
| nbascore | 🔲 Planned | — | Sports scores; needs network |
| newrelic | 🔲 Planned | — | APM; needs network + secrets |
| nextbus | 🔲 Planned | — | Transit arrivals; needs network |
| opsgenie | 🔲 Planned | — | Alerting; needs network + secrets |
| pagerduty | 🔲 Planned | — | Alerting; needs network + secrets |
| pihole | 🔲 Planned | — | DNS stats; needs network |
| ping | 🔲 Planned | — | ICMP ping; needs raw_network |
| pivotal | 🔲 Planned | — | Project tracking; needs network + secrets |
| pocket | 🔲 Planned | — | Read-later list; needs OAuth |
| power | ✅ Builtin | `power` | Battery/charge state |
| progress | 🔲 Planned | — | Generic progress bars |
| resourceusage | ✅ Builtin | `resource_usage` | CPU, memory, swap, temp |
| rollbar | 🔲 Planned | — | Error tracking; needs network + secrets |
| security | ✅ Builtin | `firewall` | Firewall status |
| spacex | 🔲 Planned | — | Launch schedule; needs network |
| spotify | 🔲 Planned | — | Now playing; needs exec or network |
| spotifyweb | 🔲 Planned | — | Spotify web API; needs OAuth |
| status | ✅ Plugin | `status-pages` | Service status pages |
| steam | 🔲 Planned | — | Game library; needs network + secrets |
| stocks | 🔲 Planned | — | Stock prices; needs network |
| subreddit | 🔲 Planned | — | Reddit feed; needs network |
| system | ✅ Builtin | `resource_usage` | Merged into resource_usage |
| textfile | ✅ Builtin | `logfile` | Same as logfile (read any text file) |
| todo | 🔲 Planned | — | Local todo list with editing |
| todo_plus | 🔲 Planned | — | Enhanced todo; needs storage |
| transmission | 🔲 Planned | — | Torrent client; needs network |
| travisci | ➖ N/A | — | Travis CI is largely deprecated |
| twitch | 🔲 Planned | — | Stream status; needs network + secrets |
| twitter | ➖ N/A | — | Twitter API deprecated |
| twitterstats | ➖ N/A | — | Twitter API deprecated |
| unknown | ➖ N/A | — | Internal wtfutil placeholder |
| updown | 🔲 Planned | — | Uptime monitoring; needs network |
| uptimekuma | 🔲 Planned | — | Uptime monitoring; needs network |
| uptimerobot | 🔲 Planned | — | Uptime monitoring; needs network |
| urlcheck | 🔲 Planned | — | URL health checks; needs network |
| victorops | 🔲 Planned | — | Alerting; needs network + secrets |
| weatherservices | ✅ Plugin | `weather` | OpenWeatherMap API |
| zendesk | 🔲 Planned | — | Support tickets; needs network + secrets |

## Summary

| Category | Count |
|----------|-------|
| ✅ Implemented | 19 |
| 🔲 Planned | 49 |
| ➖ N/A | 5 |
| **Total** | 73 |

## Slate-Only Features (not in wtfutil)

| Plugin/Feature | Type | Description |
|----------------|------|-------------|
| `ipinfo` | Plugin | Public IP geolocation (ipinfo.io / ip-api.com) |
| `brew-outdated` | Plugin | Outdated Homebrew packages |
| `istats` | Plugin | macOS hardware temps via iStats |
| `wego` | Plugin | Weather via wego CLI |
