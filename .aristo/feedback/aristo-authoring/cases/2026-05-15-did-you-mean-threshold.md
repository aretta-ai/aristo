---
date: 2026-05-15
slice: 18
file: crates/aristo-cli/src/commands/show.rs:249
id: did_you_mean_threshold_filters_noise
verdict: keep-rewrite
principles: [P-SPEC-STYLE, P-VERIFY-MATCHES-SHAPE]
verify_was: test
verify_is: neural
weak_pass: true
---

## Original (v0)

> did_you_mean caps Levenshtein distance at one third of the query length so unrelated ids never surface as 'did you mean'. A regression here would either flood the user with irrelevant suggestions (threshold too loose) or hide genuine typos (threshold too tight) — both erode trust in the suggestion as a signal worth reading.

## Better (v2)

> The Levenshtein threshold is tuned to suppress unrelated ids: too loose floods the user with noise, too tight hides real typos. Both regressions silently erode trust in the "did you mean" signal until it gets ignored.

## Why the gap

v0 pins the exact threshold ("one third of the query length") in the intent body, making it brittle (the intent rots if the threshold is tuned). v2 abstracts to "tuned to suppress unrelated ids" — the intent survives a `len/4` or `len/2` adjustment as long as the *goal* (noise suppression) is preserved.

Marked weak-pass under the content gate: the threshold is one visible line of code; the load-bearing thing is the design judgment that this number lives in a trust-vs-noise trade-off zone. Borderline; lean keep, but candidate for cleanup if it becomes noise.

## Verify level

- was: `test`
- is: `neural`
- reason: "is this threshold reasonable for noise suppression?" is a reading-the-code judgment, not a mineable runtime property. You could test specific pairs hit specific thresholds, but the qualitative "noise suppression" claim is what's load-bearing. Per P-VERIFY-MATCHES-SHAPE.
