# Plan
1. Inspect transport, SDK routes, specialized errors, REST/SSE consumers and contract generation.
2. Fix managed connector prefix; consolidate project path helper and error payload parsing.
3. Use SDK transport for scoped export and common URL builder for SSE.
4. Test prefix, scope encoding, HTTP methods, CSRF, raw response retention and invalid paths.
5. Validate current generated contracts and both frontend typechecks/build scope.
6. Document remaining hand-written endpoints and runtime acceptance; commit isolated candidate.
