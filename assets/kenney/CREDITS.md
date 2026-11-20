# Bundled asset provenance

Kenney Furniture Kit (2.0), by Kenney, https://kenney.nl/assets/furniture-kit
CC0; original archive License.txt preserved byte-for-byte as LICENSE.txt.
Selected GLBs were copied unchanged from the provided official-source archive.
See evidence/asset-inspection.json for archive/member SHA-256 and actual node,
material, accessor metadata; evidence/geometry-summary.log for compact dimensions.

Only ten exact Models/GLTF format/*.glb members and License.txt are allowed by
tools/assets.py. No general extraction, pack code execution or external resources.
Models are Y-up, offset pivots and typically half-scale relative to this office;
we apply node hierarchy transforms and a common 2x scene scale. Floor top is 0.1,
desk top about 0.869, wall top about 2.679. The doorway GLB includes a closed leaf;
it is shown as a link marker beside the open wallDoorway portal, not a simulated
hinged door. All selected materials are untextured base-color factors. Renderer
uses these factors with simple baked directional shading, not full PBR.

Source-code license remains unselected; CC0 applies to these assets only.
