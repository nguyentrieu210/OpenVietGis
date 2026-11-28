# Performance budgets & protocol
All values provisional engineering targets, not measurements. Establish baseline before changing behavior. Use exact SHA/digest, browser/OS/CPU/RAM, viewport, network (desktop broadband and mobile 4G emulation), dataset hash, provider availability, cold browser/cache and warm reload; >=30 independent repetitions for P95 lab, >=100 for P99 where practical; report sample count, failures, median/P95, bytes, peak RAM/CPU, FPS/frame time; field CWV separately when enough real samples.
| Metric | Proposed target | Measurement |
|---|---|---|
| LCP p75 / INP p75 / CLS p75 | <=2.5s / <=200ms / <=0.1 | field RUM or explicitly labeled lab proxy |
| Warm shell ready | <=1s p95 | marks route-to-interactive, assets cached |
| Initialized module transition | <=300ms p95 | no network-dependent dataset load included |
| Small authorized API read | <=200ms p95 | fixed dataset/concurrency, server latency |
| Initial compressed JS | >=30% lower if measured viable | build manifest + actual transfer |
| GIS/CAD interaction | target 60 FPS desktop, >=30 FPS mobile | frame time while pan/select |
| Eligible cache hits | >=90% repeat workload | bounded representative repeated reads, excluding noncacheable |
| Build/image size | >=40% build time / >=30% size improvement where feasible | cold and warm separate |
| Peak memory | >=20% reduction where feasible | same workload, soak 60min |
| correctness | zero tenant leaks, lost drafts, geometry regressions or P0/P1 regressions | mandatory hard gate |
Datasets: 10k/100k/1m points plus mixed polygon/line, realistic 30k house-number dataset if approved and anonymized, CAD 1k/10k/100k entities, 3D tiles under documented profile. Use synthetic/public data outside authorized production. Test network retry, reconnect, cache eviction and concurrent edits. Never turn provisional targets into passes without raw evidence. Unmet target requires reason, measured result, approved exception; no security exception.

## Final candidate execution

Finish source and no-build checks first, freeze one candidate, then run one final
build phase on GitHub and reuse its artifacts. VPS1 only deploys prebuilt artifacts;
the VPS connector must not be used for development, builds or tests.

browser-runtime.mjs now measures COUNT cold GIS contexts as well as COUNT shell
contexts. Its optional fifth argument specifies a continuous warmed-context soak
in seconds (3600 for one hour), with minute DOM/Chromium JS-heap samples. These
are not process RSS or GPU memory. Use COUNT=100 for p99 qualification.

After the final web build, invoke DIST LABEL SHA 100 3600, then run
qualification_gate.py REPORT --candidate SHA --require-soak. The checker recomputes
p95 from finite raw samples and rejects undersampling, mismatched source, exceeded
budgets and missing/discontinuous soak. It covers browser lab subcriteria only;
API/load, network, FPS, process/GPU memory and reviewed exceptions remain separate.
Historical five-sample cold GIS reports do not satisfy final sampling.
