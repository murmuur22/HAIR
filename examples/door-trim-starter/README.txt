HAIR original synthetic door-trim starter — CC0-1.0

This model was generated for HAIR from three simple rectangular trim bars; it contains no private owner model, texture or third-party geometry. It is dedicated to the public domain under CC0 1.0: https://creativecommons.org/publicdomain/zero/1.0/ .

Workshop workflow:
  Start a pack (import a Document GLB or New material-only pack).
  Choose Door trim (bounded jambs / lintel), then Import GLB… and select model.glb from this folder using the native Open panel.
  Document and Decoration retain their independent models. Add wall/floor PNGs in the materials section.
  Save NEW copy, Open its project.toml, then Export to a DIFFERENT NEW folder. The entire pack stays together.

Door trim is an explicit three-slot representation, NOT a closed door, arbitrary whole-frame replacement or door-physics system. The host frame, green interaction marker, central opening, destinations, collision and navigation remain authoritative.

Authoring contract: after GLB node transforms plus local yaw/pivot/scale, each actual triangle must be wholly in one convex normalized jamb or lintel slot. Jamb X ranges are -0.5..-0.4 and 0.4..0.5; lintel Y range is 0.9..1. All geometry must stay within X -0.5..0.5, Y 0..1, Z -0.5..0.5. Slot coordinates follow host envelope dimensions; this is not uniform whole-frame sizing. No automatic recentering or masking of invalid geometry. An opaque cube, diagonal triangle spanning slots, out-of-bounds node transform or unsafe local edit is rejected with last-good state retained.

Small rotations/pivots can be safe; large rotations or Resize object may be refused because they leave the slots. Reset returns to the general object-size fit and can also be refused; the imported starter uses canonical scale 1 for this role. File browsing, not terminal manifest editing, is the ordinary workflow.
