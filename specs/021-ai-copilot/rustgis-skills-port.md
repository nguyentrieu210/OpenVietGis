# RustGIS assistant integration — slice 1

Base a4ae85a5e3d956de1870dcf65ff3fb21da7f0367; upstream aea84131974b57ff6359666dbe79f88054a64bf1 (Apache-2.0).

## Scope and acceptance
- Port upstream skill parser and six skill definitions; adapt instructions to existing OpenVietGIS tool names and supported operations.
- MCP initialize declares prompts; authenticated prompts/list and prompts/get expose static built-ins. Unknown names and arguments rejected; no dynamic file reads or additional execution capability.
- Keep existing 39-tool registry, OAuth, approval, project pin and revision boundaries unchanged.
- Unit tests and signed OAuth HTTP regression tests added. Run cargo test --locked --manifest-path apps/mcp-rust/Cargo.toml on an approved build runner.
- Remaining: provider gateway, agent loop, ACP, vision, multi-agent and native UI integration; not included in this slice. Python adapter parity and live deployment not claimed.
