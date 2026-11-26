HAIR opened JPEG backend — parent-executed integration, review pending
=====================================================================

Status: adapter debug/release tests and actual App/Metal rendering executed;
adoptionVerified=false; HAIRsourceAcceptance=false; packageAcceptance=false.
Parent receipts and scope: external jpeg/integration-r1/PARENT-EXECUTION.md.
The file-only writer handoff below describes implementation provenance, not the
latest execution state. ONE combined all-formats/audio package remains required.

Dependency and modification notice
----------------------------------
Direct dependency: jpeg-decoder = { version = "=0.3.2-hair.1", path = "vendor-jpeg-decoder", default-features = false, features = ["platform_independent"] }.
This is a modified upstream jpeg-decoder 0.3.2, NOT the untouched official crate.
Upstream authors: The image-rs Developers; MIT copyright (c) 2016 The jpeg-decoder Developers.
Upstream repository: https://github.com/image-rs/jpeg-decoder
Official base artifact: https://static.crates.io/crates/jpeg-decoder/jpeg-decoder-0.3.2.crate
Official base artifact SHA256 (parent/review provenance, not rehashed here):
00810f1d8b74be64b13dbf3db89ac67740615d6c891f0e7b6179326533011a07
Reviewed strict fork sourcePin:
1796144ea9bf498c4916626f5f83c8b980d73d11a946b86716a34442f8db3ccd
The base checksum DOES NOT authenticate this modified fork.

Authentic licenses, unchanged in the parent-copied vendor tree:
  vendor-jpeg-decoder/LICENSE-MIT
    SHA256 22b83fb92a50418c7df9564b9c179fa2a0e74f293bc58b72dd7e218fc0902143
  vendor-jpeg-decoder/LICENSE-APACHE
    SHA256 a60eea817514531668d7e00765731449fe14d059d3249e0bc93b36de45f759f2
Upstream license URLs (references, not a network verification in this task):
  https://github.com/image-rs/jpeg-decoder/blob/v0.3.2/LICENSE-MIT
  https://github.com/image-rs/jpeg-decoder/blob/v0.3.2/LICENSE-APACHE
  https://www.apache.org/licenses/LICENSE-2.0
Retain BOTH actual authentic texts and all attribution/modification notices when
preparing the eventual source/artifact notice inventory. Do not replace them with
a license label or this document. No new package was prepared here; old generated
package notices do not establish the new dependency graph's notice completeness.
License hashes are attributed to the frozen review/parent inventories, not new
cryptographic computations by this file-only writer. Legal clearance UNVERIFIED.
Current advisory status UNVERIFIED: no dated network/advisory investigation here;
there is no claim of no advisories, blanket safety or general JPEG security.

The prior fork modifies decoder.rs, huffman.rs and worker/mod.rs behavior:
required real entropy bits, complete MCU grid, exact restart/padding/EOI, AC grammar,
checked i16 DC accumulation, inline scan and final upsample/color work. Cargo identity
and LOCAL-PATCHES.md identify modifications. Twelve additional files were formatting
only, including worker/immediate.rs: see ../jpeg/strict-decoder-r1/SOURCE-MANIFEST.json,
FROZEN-DIFF.patch and review/{REVIEW.md,VERDICT.json} for the complete 17-file diff.
LOCAL-PATCHES.md is an earlier historical handoff, not the later execution verdict.
The integration writer did not change ANY vendor file. Any necessary fork change
requires a new candidate identity/review, not a quiet modification here.

Adapter order and scope
-----------------------
The sole production entry assets/jpeg.rs::decode receives the already held immutable
byte slice. Its full preflight finishes through terminal EOI/no trailer BEFORE
constructing Decoder or calling read_info. Existing encoded4MiB, axes1..1024,
RGBA4MiB, marker4096, header256KiB, ancillary128KiB, unique nonzero 8-bit DQT,
bounded incomplete baseline DHT, SOF0 precision8, ordered gray1 or YCbCr1/2/3,
sampling1..4/max10 MCU blocks, all-component single sequential scan, APP0 JFIF-only,
bounded APP1..15, entropy framing and no-trailer predicates remain enforced.
The inventory/layout successor admits opaque APP payloads under the aggregate
128KiB ancillary limit. A bounded TIFF IFD0 parser reads only EXIF orientation
(1..8, little or big endian, at most 1024 entries, one EXIF segment); it follows
no thumbnail/sub-IFD pointers. Orientations are applied directly while writing
RGBA from samples, with swapped dimensions where required and no extra image.
ICC and Adobe rendering signatures remain explicitly refused. Sampling
is saved in a fixed three-element structure in that SAME validated SOF0 branch;
There is no second JPEG framing parser, path authority or encoded clone; the
small EXIF parser reads only slices of already bounded APP1 payloads.

Then calculate/check the logical budget below, construct Decoder<Cursor<&[u8]>>,
set_max_decoding_buffer_size(exact S) and explicit Grayscale/YCbCr, read_info,
require exact dimensions/DctSequential/L8-or-RGB24, decode, recheck identical info
and exact sample length, drop Decoder, fallibly try_reserve_exact(A), replicate L8
or copy RGB24 with alpha255, promptly drop samples, check exact RGBA length/stride.
No metadata accessor, scale, native/FFI call, native fallback or reencode exists in
this Rust path. Alpha255 is ordinary conversion, NOT an entropy-completion oracle.
Explicit YCbCr input is not permission to interpret RGB24 output as direct RGB input.
Samples are assumed sRGB; no ICC management or exact ImageIO color equivalence.
DC uses checked i16 representation, not physical-FDCT validity; IDCT wrapping and
clipping remain. No corruption/authenticity or broad color-accuracy guarantee.

The original native fixture test is renamed implementation-neutral and is no
longer macOS-cfg-limited. Original pixel coordinates/colors/tolerance48, dimensions,
full COM output equality and opacity assertions are preserved; no fixture changed.
Gray and budget tests are additional tests, not substituted fixture oracles.

Logical payload budget and T justification
------------------------------------------
All ceil/add/mul/sum operations in these terms use checked usize helpers BEFORE
Decoder::new/read_info/decode. The limit includes equality at 32MiB:
  E=held encoded byte length; N=W*H; c=1 or3; HM=max hi; VM=max vi
  MX=ceil(W/(8*HM)); MY=ceil(H/(8*VM)); BXi=MX*hi; BYi=MY*vi
  P=sum(BXi*BYi*64) sample planes
  R=sum(BXi*vi*64*2) one i16 coefficient row-set
  S=c*N decoded samples; A=4*N RGBA
  L=c*max(ceil(W*hi/HM))*HM upsample line payload
  E+P+2*R+S+A+L+T <= 32MiB; T=1MiB
The 2R term allows replacement coefficient rows while old rows are processed.
Gray's plane reuse and mutually exclusive phases are deliberately overcounted.

T is a conservative logical payload allowance for this exact codec and admission,
not just an unexplained constant. Source basis: parser.rs parse_dht/parse_dqt/
parse_com/parse_app, huffman.rs HuffmanTable::new/derive_huffman_codes,
decoder.rs table merges/SOF/SOS/scan, worker/{mod,immediate}.rs, upsampler.rs.
Conservative disjoint allowances within T (not observed allocator measurements):
  256KiB header/scratch allowance: equal to the entire preflight header cap.
    This also covers the vendor APP1 parse buffer (at most 65533 bytes), including
    when it coexists with a retained EXIF/XMP copy. No encoded-image clone occurs.
  128KiB COM/ancillary allowance: aggregate preflight cap; retained EXIF/XMP/PSIR
    plus a new ancillary result cannot exceed the total admitted payload bytes.
    APP1's temporary source buffer is additionally covered by header/scratch.
    APP0 uses a five-byte stack probe. ICC and Adobe color signatures and MJPEG
    default-table expansion remain unreachable under preflight.
  128KiB Huffman allowance: at most eight unique admitted class/id tables total.
    Conservatively allow 24 table slots across resident, parse staging, merge or
    temporary copies. Each table's logical fields fit 4KiB: <=256 value bytes,
    16 i32 deltas plus16 i32 maxcodes,256 two-u8 LUT entries and optional256
    (i16,u8) AC entries (budget4 bytes each), plus control fields. Construction
    adds <=256 u16 codes,<=256 u8 lengths,<=256 input symbols and local LUTs;
    an additional8KiB scratch fits these construction temporaries. No duplicate
    table redefinitions survive unchanged preflight. Selectors2/3 before SOF may
    exist but cannot multiply this eight-table bound.
  16KiB DQT allowance: four resident64*u16 payloads, four staging arrays,
    unzigzag temporary and Arc payload/control reservations, including replacement
    temporaries. Four unique tables only; quantization references share Arcs.
  128KiB small-control allowance: <=3 components (worker arrays sized4), frame
    clones, SOS's three index vectors, scan component clone, plane/row Vec
    descriptors, sample-factor vectors, finished flags, Huffman state, IDCT block
    and scalar temporaries, boxed upsampler objects and short diagnostic payload.
    Their fixed field counts are far below this deliberately loose allowance;
    marker count does not create a retained per-marker list. No progressive full
    coefficient image or lossless work is admitted. No decoder-owned thread stack
    or queue is created by the reviewed synchronous dispatcher.
  128KiB temporary replacement/scratch allowance: extra table move/construction
    overlap beyond the above, row-line Vec template/cloning and reader skip
    scratch. max component width<=1024, HM<=4, so one extra upsample line<=4096
    bytes. Actual c lines are already L, full sample planes P and coefficient
    replacement2R, not hidden here. No metadata expansion/encoded clone.
These conservative allowances together fit below1MiB with remaining slack. They
bound logical field/payload estimates, not arbitrary ABI padding, Vec capacity,
allocator size classes, stack reservation, std internals or process memory.
Ordinary codec Vec allocations remain infallible allocations and can abort on OOM.
The codec setter runs late after plane/row allocation: it is defense-in-depth on S,
NOT a preallocation or aggregate allocator cap. Outer RGBA reserve alone is fallible.
No measured peak/RSS/native heap/GPU/app hard cap or hard deadline is established.

Ownership unchanged and remaining gates
--------------------------------------
One existing owned outer worker keeps its reservation until retirement/join,
checks cancellation before/after acquisition and decode, and uses existing
Arc-token stale suppression and held ReaderFile authority. No new thread/queue/
retry; cancellation cannot interrupt in-progress decode. Existing raw4096 and
whole-body4MiB guards unchanged. In this successor, before-E accepts raw PNG/JPG/
JPEG suffixes only and requires the held-byte signature to match the suffix's
format before signature dispatch. Opened-reader dispatch remains signature-only.
The room scheduler reserves32MiB of logical decode payload plus a separate8MiB/
8-item cache (40MiB logical scheduler ledger); this is not allocator/RSS/GPU or
application-wide measurement. Current-room queue/source/generation checks and
cancelled-worker slot ownership are preserved. No autoplay/default app/network/
user writes were added. Parent actually exercised original JPEG object RED then
GREEN at both sizes, E/raw/same-handle C/Escape rehydration, source/envelope and
nonmutation controls. See external r2 jpeg-object/PARENT-EXECUTION.md.

Parent removed native/local_jpeg.c and build.rs only after actual replacement
GREEN, first checking both were byte-equal to the preserved integration baseline.
NATIVE-CLEANUP.json records their exact hashes. The post-cleanup JPEG App/Metal test,
PNG opened/object/zoom tests, broad suites and strict lint pass. Cargo.lock was
regenerated offline by Cargo, not handwritten; feature/advisory inventory and
independent integration source acceptance remain separate outstanding gates.

Known refusal subset: baseline Huffman selectors2/3 can pass framing but be refused
by codec parser, and noninteger sampling can be refused (apart from existing
one-pixel special cases). No widening to progressive/lossless/multiscan/CMYK/custom
IDs, APP profiles/EXIF/ICC/Adobe, thumbnails, scaling or native fallback.

Completed in preserved r1: offline lock generation, adapter8 debug/release tests,
original JPEG opened App/GPU normal/small and parent pixel inspection, PNG opened/
object/zoom regressions, broad visitor/Workshop suites, strict lint, native cleanup
and independent scoped integration source-static PASS with no blockers. Review N2
noted the former stale pending list; this successor separates completed receipts
from outstanding gates rather than rewriting the old pin.

Still outstanding: nonzero chroma/edge/sampling/restart/numerical/error-retirement/
resource qualification, dated advisory and final feature/target/unsafe/notice
inventory, current expanded-object exact-source review and final source/artifact/
extracted-package acceptance. ONE all-formats/audio package still needs PDF/text/
video/static-model/audio integration, output/lifecycle/device/native-presentation/
comfort/provenance/resource/readback/delivery qualification. Neither fork nor
adapter static PASS is exhaustive JPEG proof or combined package acceptance.
