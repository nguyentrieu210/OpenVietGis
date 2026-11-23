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
