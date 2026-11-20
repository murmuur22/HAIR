// Static equirectangular sky at infinity. No position, world lighting or depth.
struct SkyRays {
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
}
@group(0) @binding(0) var<uniform> rays: SkyRays;
@group(1) @binding(0) var panorama: texture_2d<f32>;
@group(1) @binding(1) var panorama_sampler: sampler;
struct SkyOut {
    @builtin(position) position: vec4<f32>,
    @location(0) direction: vec3<f32>,
}
@vertex fn sky_vs(@builtin(vertex_index) index: u32) -> SkyOut {
    let corners = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0)
    );
    let p = corners[index];
    var o: SkyOut;
    o.position = vec4<f32>(p, 0.0, 1.0);
    o.direction = rays.forward.xyz + p.x * rays.right.xyz + p.y * rays.up.xyz;
    return o;
}
fn sky_rgb(direction: vec3<f32>) -> vec3<f32> {
    let d = normalize(direction);
    let uv = vec2<f32>(
        atan2(d.x, -d.z) / 6.283185307179586 + 0.5,
        acos(clamp(d.y, -1.0, 1.0)) / 3.141592653589793
    );
    // U repeats, V clamps at the sampler. Level zero avoids seam derivatives;
    // the bounded static image has no mips. sRGB sampling returns linear light.
    return textureSampleLevel(panorama, panorama_sampler, uv, 0.0).rgb;
}
@fragment fn sky_fs(o: SkyOut) -> @location(0) vec4<f32> {
    return vec4<f32>(sky_rgb(o.direction), 1.0);
}
@fragment fn sky_unorm_fs(o: SkyOut) -> @location(0) vec4<f32> {
    let rgb = sky_rgb(o.direction);
    let encoded = select(12.92 * rgb, 1.055 * pow(rgb, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055), rgb > vec3<f32>(0.0031308));
    return vec4<f32>(encoded, 1.0);
}
