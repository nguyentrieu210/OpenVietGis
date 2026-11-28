# RustGIS attribution

Source: https://github.com/opengeos/RustGIS/tree/aea84131974b57ff6359666dbe79f88054a64bf1/crates/assistant
License: Apache-2.0 (full text in RustGIS-LICENSE.txt).
The original skill front matter and parser are derived from the RustGIS project and its contributors. OpenVietGIS modified the skill bodies for its own registry and tenant/consent policy; only built-in skills are exposed. No upstream desktop command or provider credentials are imported. See source URLs for history.

Wave 2: assistant/agent.rs conversation-loop design, step/retry limits and comments adapted from the same pinned upstream. OpenVietGIS replaces its desktop Session and provider types with an authenticated workspace dispatcher and provider-neutral host callbacks.
