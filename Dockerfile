# Runs the Lugh MCP server (stdio). Used by MCP directories such as Glama.
FROM node:22-alpine
RUN npm install -g lugh-lang@latest
ENTRYPOINT ["lugh-lang", "mcp"]
