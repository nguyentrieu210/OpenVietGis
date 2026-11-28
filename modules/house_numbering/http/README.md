# House Numbering HTTP adapter

This crate owns the existing Axum house-numbering routes and request/response contracts. The application composition root supplies ModuleHttpState via the generic ovg-module-http crate. The host is responsible for authenticated browser sessions and CSRF validation, and preserving Core project-access error semantics. Service principal authentication remains in the authorized storage API.

The module does not import the HTTP application, own the main server, or instantiate a separate database connection. Project authorization, activation compatibility, module permission restrictions, and transactional RLS remain mandatory. HTTP URLs and existing module migrations are unchanged.

Run the module router tests and isolated HTTP/PostGIS acceptance harness from tests/plugin-runtime/bindings-http-smoke.py with --pure-commands --module-events.
