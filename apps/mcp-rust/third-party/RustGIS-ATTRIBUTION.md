# RustGIS attribution

Source: https://github.com/opengeos/RustGIS/tree/aea84131974b57ff6359666dbe79f88054a64bf1/crates/assistant
License: Apache-2.0 (full text in RustGIS-LICENSE.txt).
The original skill front matter and parser are derived from the RustGIS project and its contributors. OpenVietGIS modified the skill bodies for its own registry and tenant/consent policy; only built-in skills are exposed. No upstream desktop command or provider credentials are imported. See source URLs for history.

Wave 2: assistant/agent.rs conversation-loop design, step/retry limits and comments adapted from the same pinned upstream. OpenVietGIS replaces its desktop Session and provider types with an authenticated workspace dispatcher and provider-neutral host callbacks.

Wave 3: provider.rs protocol adapter design, native-message replay, provider retry classification and fixed presets adapted from the same pinned source; OpenVietGIS server transport and consent/tenant boundary are new.

Wave 4: assistant/agent.rs subagent delegation, separate conversation and shared budget patterns adapted from the pinned upstream. OpenVietGIS uses concurrent read-only specialists and serialized tenant-scoped dispatch.

Wave 5: visual map observation and native image-block design adapted from RustGIS assistant/tools.rs and provider.rs at the pinned revision; OpenVietGIS uses its own bounded canvas capture and explicit consent.

Wave 6: completed-group compaction, UTF-8 trimming and overflow classification concepts adapted from assistant/compact.rs at the pinned upstream. OpenVietGIS uses a deterministic receipt projection and stable provider-native call keys.
