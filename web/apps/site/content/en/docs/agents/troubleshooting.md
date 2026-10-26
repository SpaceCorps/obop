---
title: "Troubleshooting"
description: "Fix common problems when connecting agents to obop: MCP client shows no tools, 401 and 404 answers, invalid token, unknown field on filters, approval required, and database path mistakes."
---

## The MCP client shows no tools, or says the server failed

1. **Run the server by hand.** Use the handshake command from [Connect over MCP](/docs/agents/mcp/). If it prints a JSON answer, the server is fine and the client config is wrong. If it prints an error, fix that first.
2. **Use absolute paths.** Clients start the server from an unknown working directory. Use the full path to `obop` and an absolute database path: `sqlite:///home/me/obop.db` (three slashes, then the absolute path).
3. **`no valid token`.** The message reads `no valid token: pass --token (or OBOP_TOKEN); see obop admin create and obop agent-token`. The token is missing, wrong, revoked or its owner was deleted. Create a new one.
4. **The database is empty or new.** If `database: sqlite` points at a different file than your instance, you see no users and no records. Check `OBOP_DATABASE_URL`.
5. **Restart the client.** Most clients read the config only at start.
6. **Read the client's log.** Claude Desktop writes `mcp.log` and one log per server (macOS: `~/Library/Logs/Claude`). The server prints `database: sqlite` on stderr when it starts; that line is normal.

## The agent sees fewer tools than expected

The list is the commands the token may use. Add the group that the missing command needs (create a new token), or check the model's `read_group` and `write_group` on the [app page](/apps/). Hidden and forbidden commands are indistinguishable from unknown ones (`404` or MCP `unknown tool`) on purpose.

## REST answers

| You get                                            | Likely cause                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `401`                                              | No `Authorization: Bearer ...` header, or a wrong, expired or revoked token. Session tokens last 12 hours; agent tokens do not expire.                                                                                                                                                                                                                                                                                                                                                                          |
| `404 {"error":"not found"}`                        | Wrong model or command name, the token lacks the group, or the command is not on this surface.                                                                                                                                                                                                                                                                                                                                                                                                                  |
| `415`                                              | Send `Content-Type: application/json`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `400 {"error":"invalid token"}`                    | A masking token was not recognised: it is from another session or an older server process. Read the record again.                                                                                                                                                                                                                                                                                                                                                                                               |
| `400 {"error":"unknown field party.email"}`        | You filtered or sorted on a field the agent cannot read in clear (masked, withheld or not permitted).                                                                                                                                                                                                                                                                                                                                                                                                           |
| `400 {"error":"lead.title is required"}`           | A required field is missing.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `400 ... starts as "new" when a record is created` | You tried to set a governed state on create. Leave it out and use an action.                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `400 ... cannot change from "new" to "won"`        | No action moves it that way. The message lists where the record can go.                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| `429`                                              | Five failed sign-ins for a username lock it for a minute.                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `202`                                              | Not an error: approval is required. See [The approval loop](/docs/agents/approvals/).                                                                                                                                                                                                                                                                                                                                                                                                                           |
| `503 {"error":"storage_busy"}`                     | The instance's database is busy (also `storage_unavailable`, `storage_timeout`). Wait the seconds in the `Retry-After` header and try again, after reading `applied` in the body: `none`, the call changed nothing; `unknown` (only after `storage_unavailable`), a change in flight may or may not have been applied, so read before you repeat a write; `partial`, part of the call was done and a step it needed was refused, so do not repeat it blindly: read what it touched and finish or undo the rest. |

## An approval plan was lost

Pending plans live in the server's memory and disappear when the server restarts. Ask the agent to propose again.

## `approve` returns 404

The plan id is unknown or was already used, an agent tried to approve (only a person can), or the approver does not hold the action's group.

## The agent keeps asking for data it cannot see

Tell it in its instructions that personal data is masked, that withheld fields are not available, and to report instead of asking. See [Masking tokens](/docs/agents/masking/).

## Local development quirks

- `--dev-auth` swaps real sign-in for fixed tokens and only binds to localhost. Do not use it for real agents.
- A SQLite file is opened by every process that points at it. `obop serve` and `obop mcp` can share one file on one machine.
- The CLI prints `database: sqlite` first on every run. That line goes to stderr and does not end up in piped output.

## Still stuck

Check the [FAQ](/docs/reference/faq/), search the [issues](https://github.com/SpaceCorps/obop/issues) or ask in the [community](/about/community/).
