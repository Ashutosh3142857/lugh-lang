// A tiny MCP server for the guard tests. Tools echo what they received, and
// `calls_seen` reports every call that actually reached the server.
const tools = [
  { name: "list_items", description: "List items", annotations: { readOnlyHint: true }, inputSchema: { type: "object" } },
  { name: "add_item", description: "Add an item", inputSchema: { type: "object" } },
  { name: "delete_item", description: "Delete an item", annotations: { destructiveHint: true }, inputSchema: { type: "object" } },
  { name: "legacy_export", description: "Old tool without annotations", inputSchema: { type: "object" } },
  { name: "calls_seen", description: "Calls that reached the server", annotations: { readOnlyHint: true }, inputSchema: { type: "object" } },
];
const seen = [];
let buf = "";
process.stdin.on("data", (d) => {
  buf += d;
  let i;
  while ((i = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, i);
    buf = buf.slice(i + 1);
    if (!line.trim()) continue;
    const msg = JSON.parse(line);
    const reply = (result) => process.stdout.write(JSON.stringify({ jsonrpc: "2.0", id: msg.id, result }) + "\n");
    if (msg.method === "initialize") reply({ protocolVersion: "2025-06-18", capabilities: { tools: {} }, serverInfo: { name: "fake", version: "1" } });
    else if (msg.method === "tools/list") reply({ tools });
    else if (msg.method === "tools/call") {
      if (msg.params.name !== "calls_seen") seen.push(msg.params.name);
      const text = msg.params.name === "calls_seen" ? JSON.stringify(seen) : `ran ${msg.params.name} ${JSON.stringify(msg.params.arguments || {})}`;
      reply({ content: [{ type: "text", text }] });
    } else if (msg.id !== undefined) reply({});
  }
});
