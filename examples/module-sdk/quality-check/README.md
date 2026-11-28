# Quality Check — independent native SDK example

This opt-in example is intentionally outside the production marketplace discovery directory. It imports only the Core domain and native module SDK. It has no database credentials, application internals, or house-numbering dependency.

Compile from the repository root:

    cargo test -p ovg-module-quality-check
    cargo build -p openvietgis-api-rust --features sdk-example

The application composition root registers QualityCheck when that feature is selected. The default production build does not include the example. A trusted maintainer must review and compile native code; this is not a runtime upload or sandbox.

## Exercise in a disposable test environment

Register the manifest using POST /api/v1/organizations/{org}/extensions/official/quality_check/register and activate its returned manifest ID using POST /api/v1/organizations/{org}/projects/{project}/extensions/activations. Use the authorized browser client and CSRF header. The project manager then binds a ready Point source containing field code to role survey, with its exact schema revision.

The module runtime contract, bound spatial summary and pure completion-rate command are exposed by the generic Core API. A summary with source_count=0 means no eligible bound source, not completed survey. The geometry_present count describes geometry availability, not fieldwork completion. The completion_rate command operates solely on supplied counts and does not persist or certify them.

    python tests/plugin-runtime/bindings-http-smoke.py --candidate target/debug/openvietgis-api-rust --pure-commands --module-events --sdk-example

See docs/native-module-sdk.md for boundaries, versioning and lifecycle.
