Local source patch identity: jpeg-decoder 0.3.2-hair.1
===================================================

UNSHIPPED, FILE-ONLY CANDIDATE. Compile, runtime and independent review pending.
Not the authentic unmodified 0.3.2 release; not approved for HAIR integration.

Base provenance
---------------
Parent supplied a byte-equal copy of the official artifact's extracted members.
Official .crate SHA256:
00810f1d8b74be64b13dbf3db89ac67740615d6c891f0e7b6179326533011a07
../BASELINE.json records the original member hashes. No new hash computation
was possible with the authorized tools. Parent must hash the final exact tree,
retain a diff against BASELINE.json, and review that identity before execution
receipts are used for adoption. Neither the artifact nor the original external
crate, old reviews, spike, native failures or source fixture was edited.

The active normalized Cargo.toml identifies 0.3.2-hair.1. Cargo.toml.orig,
.cargo_vcs_info.json, Cargo.lock, README and changelog are retained historical
upstream material, NOT attestations of this patch or a current fork lockfile.
Both authentic LICENSE texts and copyright text are unchanged. This file is
prominent local change identification; final notice/compliance is a parent gate.

Modified upstream files
-----------------------
Cargo.toml
    Distinct prerelease version only, with local-change comment. No new deps.
src/huffman.rs
    Separate real_bits from padded lookahead; every code/magnitude consumption
    is fallible and must fit real_bits, including combined fast-AC consumption.
    Exact boundary extraction validates 0..7 all-one pad bits and an immediate
    marker; no scanning past entropy. FF stuffing and marker-fill distinguished.
src/decoder.rs
    Guard SOF0/8-bit/conventional gray or YCbCr, one all-component sequential
    scan. Exact MCU-grid traversal. Require EOI immediately after final segment;
    no terminal RST tolerance or swallowed scan-end marker error. Baseline AC
    runs/ZRL/categories checked; DC accumulation checked rather than wrapped.
    Both worker preferences immediate; unused cmp import removed.
src/worker/mod.rs
    Immediate-only worker storage and serial final color/upsample pass. No
    multithreaded or Rayon module declaration/dispatch, regardless of width or
    feature preference. Historical source files remain but are NOT compiled.
    Removed obsolete asmjs cfgs along with their now-disabled worker branches;
    no lint suppression or fake target configuration was introduced.

Scope and exclusions
--------------------
This is an always-strict narrow baseline fork, not a configurable tolerant mode.
Full unchanged HAIR preflight is STILL REQUIRED, including all dimension, byte,
header, table, ancillary, APP/profile and terminal-EOI/no-trailer checks. The
fork does not itself implement those application caps or replace preflight.

Legacy progressive/lossless code remains typechecked but is unreachable through
Decoder after the new SOF guard. In particular decoder/lossless.rs still contains
its historical read_marker().ok() recovery: it is not repaired or claimed safe.
The general header read_marker remains permissive; unchanged HAIR preflight
excludes those extraneous header bytes. Entropy termination no longer calls it.
Re-enabling other profiles would invalidate this narrow review scope.

Default features of upstream Cargo.toml remain historically Rayon-enabled;
the only prepared caller explicitly uses default-features=false and
features=["platform_independent"]. Even an enabled Rayon feature has no worker
or final-color dispatch in this fork, but selected dependency/features still
require parent inspection. Unchanged upstream Rayon tests are not this fork's
acceptance graph. Run only the isolated probe graph initially.

DC/category policy
------------------
Baseline DC difference categories 0..11, sign extension as upstream; category 0
is zero, others represent signed nonzero differences with magnitude <=2047.
Predictor is an i16 with checked accumulation, reset to zero at each required
restart. Unrepresentable sums error; no modular predictor or recovered output.
There is deliberately no invented tighter bound based on quantizer, reconstructed
samples, or an assumed exact physical FDCT. This is a numerical representation
policy, not certification that every representable coefficient stream is an
encoder's realizable 8-bit FDCT output. Fresh review must assess this choice.
Baseline AC magnitudes 1..10, EOB=00 and ZRL=F0 only for zero-size symbols;
nonzero runs must land in 1..63; ZRL may end exactly at 64, never beyond it.
Completing coefficient 63 needs no additional EOB. No progressive EOB runs.
Existing IDCT/color clipping and wrapping implementation is unchanged; this
patch does not promise physical-coefficient validation or exact ImageIO colors.

Execution status
----------------
Tests prepared, NOT executed. See ../IMPLEMENTATION.md and ../RESULT.json.
No installation, HAIR edits, lockfile regeneration, shipping, source/package
acceptance, current advisory clearance or all-format safety claim.
