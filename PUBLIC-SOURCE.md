# Public product-source export

This branch tracks HAIR 0.4.1 product code, runtime resources, ordinary tests and vendored dependency notices. It intentionally does not contain private investigation history, qualification logs, local orchestration, owner notes or old build artifacts. The project-wide source license has not yet been selected; dependency and asset notices retain their own terms.

Runtime source and resources match the owner-test candidate. Two test-only export adaptations are explicit: the ignored local-world sparse GPU supplement and its unpublished external test modules are omitted; the audio ended-order test still requires an explicitly owned qualification workspace, named by HAIR_TEST_WORKSPACE rather than a developer-specific absolute path. Example HOW-TO-RUN files referring to private historical build locations are omitted. These adaptations do not affect the application binary.

Follow README.md for the release build. The ordinary suite includes qualification fixtures requiring explicit owned temporary/workspace roots, and some native/media tests need a synthetic video fixture. A successful release build or selected portable tests does not imply the complete private qualification environment is available from this export. Ignored GPU/native tests require separate deliberate execution; do not remove fixture guards merely to obtain a green suite.

This is test-branch source, not a public binary release, notarization or distribution/legal clearance.
