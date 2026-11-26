# HAIR 0.4.4 work-in-progress source checkpoint

This branch tracks ongoing development. It is **not a release, accepted test build, or a replacement for the delivered 0.4.3 private trial**. Package metadata still says 0.4.3; the intended successor milestone is 0.4.4.

## Implemented source

- Persistent Documents collection, metadata listing, cold reopen and explicit healthy removal.
- Storage settings with independent disk limits and explicit compatible-cache selection.
- Bounded background document import and selected-preview jobs with cancellation, stable-ID admission, retained cache authority and truthful late-commit handling.
- Placement/cache/Resources namespace protections, including recovery publication; Resources identity retained from bootstrap.
- Corrections preventing ordinary UI inspection from cancelling durable imports, plus bounded post-loop Documents settlement.
- In-progress typed preference-write outcomes: distinguish precommit refusal, committed directory-sync warning and unknown activation. Adoption preflights blocked preferences; late failure discloses an initialized candidate without deleting it.

## Verification and limits

The latest settings correction passed the all-features Documents App module: **48 passed, 0 failed**. Its three newly reproduced input regressions failed before the correction and passed afterward. Earlier async lifecycle changes passed independent source review, with retained Low caveats.

The newest typed preference-outcome changes are **not independently reviewed** and do not yet have full fault-injection coverage. Full suites, strict lint after the newest settings edits, graphics/native execution and exact-package qualification remain open. Earlier results apply only to their earlier source snapshots. A checkpoint commit is not evidence of whole-feature acceptance.

## Remaining milestone work

- Complete preference activation/durability fault tests and review.
- Remaining synchronous storage metadata, removal and final placement export paths; explicit recovery discard UI.
- Requested Documents reader, Decorations inventory/viewer and remaining placement/animation UX.
- Native/graphics validation, complete regressions and actual package acceptance.
- Owner-reported save/reopen issue remains unreproduced and unfixed; parked separately.

## Publication scope

Only product source, tests, dependency sources, sample assets and build manifests are included in this checkpoint. Private execution logs, diary files, local handoffs, credentials, user documents and packaged binaries are not included. Development originals and accepted packages remain untouched. Some historical graphics-test/example paths are machine-specific; this snapshot is not a claim of portable qualification.
