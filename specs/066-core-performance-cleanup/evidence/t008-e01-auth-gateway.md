# T008 E01 Auth and identity gateway slice

Status: partial routing and evidence only. T008 remains open until the complete candidate is integrated and its required parity gates pass.

## Dispatch policy

| Route | Rust dispatch | Reason |
| --- | --- | --- |
| GET `/auth/login/keycloak` | No Authorization header only | Existing E01 differential coverage verifies the Keycloak browser flow. Other provider names, including configured dynamic providers such as `other`, stay on FastAPI because the static ingress cannot read the runtime OIDC provider allowlist. |
| GET `/auth/callback/keycloak` | No Authorization header only | Same verified browser flow and provider boundary as login. |
| PUT `/me/avatar` | Cookie-session candidate only | Existing Rust handler authenticates `HumanSession`, requires CSRF, normalizes the image and persists it. |
| POST `/organizations/{organization_id}/invitations/email` | Cookie-session candidate only | Existing handler requires `HumanSession`, CSRF, membership and invitation-management permission. It validates the request before attempting configured delivery. |

Every other provider and any bearer authorization header use FastAPI. Nginx has `proxy_next_upstream off`; a selected Rust upstream failure returns an error and does not silently retry FastAPI.

## Semantic differential evidence

- Existing `tests/rust-parity/e01_oidc.py` compares FastAPI and Rust against a synthetic OIDC provider. It covers Keycloak login/callback, PKCE, cancel, password action and identity-link behavior. It also configures an `other` provider and exercises an unknown provider; gateway dispatch deliberately leaves both on FastAPI.
- Existing `tests/rust-parity/e01_e10_e12_wave.py` compares avatar normalization, persistence, missing-CSRF rejection and invalid-image rejection.
- The email invitation route was audited against the FastAPI handler and Rust's `HumanSession`, CSRF, request validation, permission and mail-configuration checks. A successful external delivery differential was not added or run; Rust uses a fixed Resend destination and must be qualified separately without sending real email.

## Gateway smoke result

Command: `python3 tests/rust-parity/e01_auth_gateway_smoke.py`

Result: passed 18 dispatch/header/body cases and 5 outage/fallback cases. Nginx config syntax passed. The smoke sent no real OAuth callback, email invitation or other external request; both upstreams were local recorders. Rust outage returned 502 for login, callback, avatar PUT and email invitation; unsupported providers remained on Python.

Python syntax parsing passed for both new and existing touched Python files. The Rust/PostGIS semantic suites were not run on this VPS because it has no built Rust candidate or Cargo installation.

## Scope

No workflow changes, merge, deployment or T008 completion check were made.
