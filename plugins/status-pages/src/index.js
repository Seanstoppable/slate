// Status Pages plugin — checks service status APIs and displays results.
// Language: JavaScript (compiled via Extism JS PDK / QuickJS)

function metadata() {
  Host.outputString(JSON.stringify({
    name: "Status Pages",
    description: "Monitor service status pages (Atlassian Statuspage, custom APIs)",
    version: "0.1.0",
    author: "Slate Community",
  }));
  return 0;
}

function getNestedValue(obj, path) {
  return path.split(".").reduce(function(acc, key) { return acc && acc[key]; }, obj);
}

function checkService(service) {
  try {
    var req = new HttpRequest(service.url);
    req.method = "GET";
    req.headers = { "Accept": "application/json" };
    var resp = Http.request(req);
    var body = resp.body;

    if (!body || body.length === 0) {
      return { name: service.name, status: "error", message: "Empty response" };
    }

    var data = JSON.parse(body);
    var statusPath = service.statusPath || "status.indicator";
    var messagePath = service.messagePath || "status.description";

    var status = getNestedValue(data, statusPath) || "unknown";
    var message = getNestedValue(data, messagePath) || "";

    return { name: service.name, status: String(status), message: String(message) };
  } catch (e) {
    return { name: service.name, status: "error", message: e.message || "Request failed" };
  }
}

function refresh() {
  var input = Host.inputString();
  var settings = {};
  try { settings = JSON.parse(input); } catch(e) {}

  var services = settings.services || [
    { name: "GitHub", url: "https://www.githubstatus.com/api/v2/status.json" },
    { name: "Slack", url: "https://status.slack.com/api/v2.0.0/current", statusPath: "status", messagePath: "date_updated" },
  ];

  if (services.length === 0) {
    Host.outputString(JSON.stringify({
      type: "text",
      content: "Configure 'services' in widget settings to monitor status pages.",
      scrollable: false,
      wrap: true,
    }));
    return 0;
  }

  var results = [];
  for (var i = 0; i < services.length; i++) {
    results.push(checkService(services[i]));
  }

  var pairs = [];
  for (var j = 0; j < results.length; j++) {
    var r = results[j];
    var icon = (r.status === "none" || r.status === "ok" || r.status === "operational") ? "✓"
      : (r.status === "error") ? "✗" : "⚠";
    pairs.push({ key: icon + " " + r.name, value: r.message || r.status });
  }

  Host.outputString(JSON.stringify({ type: "key_value", pairs: pairs }));
  return 0;
}

function on_key() {
  Host.outputString("");
  return 0;
}

function on_action() {
  Host.outputString("");
  return 0;
}

module.exports = { metadata: metadata, refresh: refresh, on_key: on_key, on_action: on_action };

