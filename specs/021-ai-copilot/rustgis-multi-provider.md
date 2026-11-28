# Wave 3 — multi-provider gateway

Base Wave 2 PR #106 at 2065ae72c4f4988915278748fae82a1f9982a91e; provider wire format design derived from RustGIS assistant/provider.rs at aea84131974b57ff6359666dbe79f88054a64bf1 (Apache-2.0).

## Scope
- OpenAI Chat Completions, Anthropic Messages, Gemini generateContent, OpenRouter and local Ollama (OpenAI compatible). Fixed URLs, no redirects, bounded response and timeout; no arbitrary endpoint, model ID or credential from clients.
- Operator config: OVG_AI_ALLOWED_SUBJECTS (comma-separated exact Keycloak subject IDs), OVG_AI_OPENAI_MODEL/KEY, OVG_AI_ANTHROPIC_MODEL/KEY, OVG_AI_GEMINI_MODEL/KEY, OVG_AI_OPENROUTER_MODEL/KEY, OVG_AI_OLLAMA_MODEL. Ollama fixed loopback port 11434; no keys in source, logs, responses or browser. Unconfigured accounts see no providers.
- Provider-native assistant replay preserves Gemini signatures and Anthropic opaque content within one run; tool outputs serialized as observations. No hidden reasoning returned to browser.
- Authenticated first-party provider discovery and asynchronous run under /ai-copilot/agent/{session}/providers, /run, /status and /cancel, explicit per-run consent and pinned provider/model. Browser identity rechecked before every model request and GIS dispatch. Default read-only; writes opt-in and still individually approved; plugin administration excluded.
- One active run per subject, four globally per process. Bounded run, no automatic replay of failed GIS operations. Browser cancel closes workspace and aborts request; a dispatched operation may remain uncertain. Bounded 64-entry process-local job registry, 15-minute completed-result retention, explicit cancellation and 202/status polling; no durable job recovery or cross-process rate limiter.
- Existing MCP protocol and external client tools unchanged. No ChatGPT consumer OAuth or subscription reuse.
- First-party panel uses same session/review controls. Native gateway must be routed by reverse proxy with ordinary short request timeout, and browser/API contract must be tested before rollout. Legacy Python gateway has no new endpoints.

## Acceptance
Compile and run native MCP tests; provider fixture roundtrips, invalid responses and fixed destinations; run frontend smoke with disabled provider configuration and with a controlled mock before live release. Real provider tests need explicit operator credentials and authorized non-sensitive fixture data. No live rollout or real model invocation is claimed.

Wave 3 hardening: candidate nginx include, same-instance routing requirement, signed-browser fixture rejects cross-workspace tokens, missing consent and client-supplied keys/URLs. Browser smoke covers disabled provider, mock configured provider, per-run consent and response text. Context changes terminate active native run; revoked operator allowlist blocks further tool dispatch. Proxy/CDN compatibility remains a deployment gate.

The asynchronous start/status protocol avoids holding open a 300-second HTTP request through Cloudflare. Status and cancel revalidate exact workspace owner; the worker retains its subject slot and rechecks permissions. A process restart loses job status, so never automatically replay an unknown operation.
