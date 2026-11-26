# Security policy

Please report security problems privately, not in a public issue:
use **Report a vulnerability** on the repository's
[Security tab](https://github.com/Ashutosh3142857/lugh-lang/security/advisories/new).

Useful reports include a policy bypass in guard (a call that should have been
denied or asked reaching the MCP server), a way for a Lugh program or a tool
result to escape the sandbox, or anything that writes outside the files
`protect` says it will change.

You'll get a reply within a few days. Fixes ship as a new npm and crates.io
release, and the advisory credits you unless you ask otherwise.

Supported versions: the latest release.
