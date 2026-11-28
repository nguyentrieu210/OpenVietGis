# 066 implementation slice 15 — cache headers, validators and private failure policy

Parent candidate: 1862bc9fa006f0949b8c3a59d1332e2bcb4b1a26.

## T025 — cache headers / ETag / 304 / provider boundary
- Read-only public HTTPS observation of the current live static frontend records a hashed JS asset with Cache-Control public,max-age=31536000,immutable, an ETag, Cloudflare HIT and a successful conditional ETag 304. The root HTML remains dynamic at the edge.
- Repository Nginx policy applies immutable one-year caching only to content-addressed asset paths; HTML, service worker and non-hashed shells revalidate. The offline shell test separately proves protected workspace/optional engine chunks and private API responses are not placed in the service-worker cache.
- The current live unauthenticated /api/v1/me response was dynamic but lacked an explicit Cache-Control header. The candidate fixes this in the global request middleware: 401/403/404 responses receive private,no-store unless a route already supplied a stricter cache directive. This also covers anti-enumeration 404 responses.
- Rust route contract test PASS on the pinned 1.85.1 toolchain: the existing 404 error-envelope test now also asserts private,no-store. Existing route-specific successful private GIS/tile/download responses keep their explicit no-store policies.
- External/provider imagery is not copied into the offline service worker. Professional authorized tiles remain private/no-store and revision-pinned; external basemap requests remain governed by provider/browser behavior rather than being promoted to the platform immutable cache.
- The evidence file live-cache-headers.json contains only public response metadata and no credential or tenant data.
- This closes T025 for the candidate. It does not claim the private-header fix is live; post-release verification belongs to T048.

No CDN setting, production API or live container was modified by this slice.
