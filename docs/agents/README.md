# OpenVietGIS Agent Kit — five isolated workstreams

This kit does not include an AI model or credentials. It prepares five isolated Git
worktrees and task contracts for authenticated Codex/Claude CLI agents, with a
single-run lock to protect a low-memory server. Spec Kit remains the governing
specification workflow; this kit is its worktree/ownership dispatch layer.

## Workstreams

| ID | Track | Task contract | Own output |
| --- | --- | --- | --- |
| 01 | Git | 01-git-governance.md | docs/qa/agent-01-git.md |
| 02 | Frontend | 02-frontend-parity.md | docs/qa/agent-02-frontend.md |
| 03 | Backend | 03-api-plugin-migrations.md | docs/qa/agent-03-backend.md |
| 04 | Release | 04-release-rollback.md | docs/qa/agent-04-release.md |
| 05 | QA | 05-qa-acceptance.md | docs/qa/agent-05-qa.md |

## Initialize on VPS (safe to repeat)

~~~bash
cd /opt/openvietgis-control-kit-20261123
python3 tools/agent_kit.py prepare \
  --repo /opt/openvietgis-control-kit-20261123 \
  --base maintenance/control-kit-20261123 \
  --root /opt/openvietgis-agent-worktrees
python3 tools/agent_kit.py status --root /opt/openvietgis-agent-worktrees
~~~

Each lane is a real Git worktree plus a distinct agentkit/<track>-20261123
branch. They all start from the same committed base SHA, not current dirty code.

## Execute after CLI authorization/quota is available

The VPS currently has neither codex nor claude in PATH, so execution on the
VPS is expected to refuse, while preparation/status/tests still work.
Do not copy a browser OAuth token, API key or CLI session from Windows to VPS.
Instead run the same kit against a clean local clone on an authenticated host,
or install/authenticate an agent CLI securely on the target host.

Dry-run (no AI invoked):

~~~bash
python3 tools/agent_kit.py run --track frontend --engine codex
~~~

Explicit real execution (one agent at a time):

~~~bash
python3 tools/agent_kit.py run --track frontend --engine codex --execute
~~~

Claude engine is optional via --engine claude. If model usage quota is exhausted
the engine exits with a failure; the kit does not retry or bypass account limits.
Logs and lock are stored outside Git under the agent root. Review logs for
privacy before sharing. On machines with limited free memory, keep concurrency 1.

## Review and merge gate

1. Agent edits only its own worktree. Re-run status to inspect dirty paths
   and commits. A READY state means no changes, not task completion.
2. Review output and diff on the relevant agentkit/* branch, and run
   lightweight checks. Production tests must use a safe test environment.
3. Open an individual PR for each completed, independently validated lane.
4. Do not auto-merge, push or deploy from this kit. Run CI, security review and
   acceptance before a human approves merge. Production releases remain pinned
   to immutable tested SHA with rollback evidence.

## Safety constraints

- No deletion, force-push, resets, database operations or service changes.
- No writable shared worktree across agents.
- No unverified assumption that a public route or code-level feature works.
- No secrets or long-lived cloud credentials in prompts/logs/repo.
- No future claim that five agents are running merely because five worktrees exist.

---

## R025 MicroStation V8i Bridge Agent Track

# R025 MicroStation V8i Agent Dispatch

Governing feature spec: `specs/025-microstation-v8i-bridge`. Baseline `feature/025-microstation-v8i-bridge`.

Five isolated tracks: windows, backend, frontend, qa, release. Each track owns a separate Git worktree and task contract. The installed dispatcher **prepares** agent runs but cannot execute an AI model without an authenticated `codex` or `claude` CLI.

Prepare:
```sh
python3 tools/agent_kit_v8i.py prepare --repo /opt/openvietgis-v8i-bridge --base feature/025-microstation-v8i-bridge --root /opt/openvietgis-v8i-agent-worktrees --suffix v8i-20261123
python3 tools/agent_kit_v8i.py status --root /opt/openvietgis-v8i-agent-worktrees
```

Once an authorized agent CLI is actually installed on the execution host, run one track at a time using `--execute`. Without `--execute`, only a **dry-run** is printed. Do not copy OAuth tokens from user devices to VPS, do not alter existing live services, and do not claim the agents are actively running based on prepared worktrees.

Each agent must submit a scoped diff, verification evidence, and unresolved blockers for review. No automatic merge or deploy.
