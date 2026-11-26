struct Camera {
    view_projection: mat4x4<f32>,
    eye_enabled: vec4<f32>,
    fog_range: vec4<f32>,
    fog_tint: vec4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var base_color: texture_2d<f32>;
@group(1) @binding(1) var base_sampler: sampler;
struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) world_position: vec3<f32>,
}
@vertex fn world_vs(@location(0) p: vec3<f32>, @location(1) c: vec3<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out; o.position = camera.view_projection * vec4<f32>(p,1.0); o.color = c; o.uv = uv; o.world_position = p; return o;
}
@vertex fn ui_vs(@location(0) p: vec3<f32>, @location(1) c: vec3<f32>, @location(2) uv: vec2<f32>) -> Out {
    var o: Out; o.position = vec4<f32>(p,1.0); o.color = c; o.uv = uv; o.world_position = vec3<f32>(0.0); return o;
}
fn fogged(rgb: vec3<f32>, p: vec3<f32>) -> vec3<f32> {
    let amount = smoothstep(camera.fog_range.x, camera.fog_range.y, distance(p, camera.eye_enabled.xyz)) * camera.eye_enabled.w;
    return mix(rgb, camera.fog_tint.rgb, amount);
}
@fragment fn world_fs(o: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(fogged(o.color * textureSample(base_color, base_sampler, o.uv).rgb, o.world_position), 1.0);
}
// Inventory/inspector textures use the same materials, never world fog.
@fragment fn texture_fs(o: Out) -> @location(0) vec4<f32> {
    return vec4<f32>(o.color * textureSample(base_color, base_sampler, o.uv).rgb, 1.0);
}
@fragment fn ui_fs(o: Out) -> @location(0) vec4<f32> { return vec4<f32>(o.color,1.0); }
// Rgba8UnormSrgb samples are converted to linear by the shared material. Alpha
// is straight PNG alpha, composited in linear space onto the viewer background.
@fragment fn media_fs(o: Out) -> @location(0) vec4<f32> {
    let texel = textureSample(base_color, base_sampler, clamp(o.uv, vec2<f32>(0.0), vec2<f32>(1.0)));
    return vec4<f32>(mix(vec3<f32>(0.025), texel.rgb * o.color, texel.a), 1.0);
}
// Non-sRGB SDR surfaces do not apply the framebuffer transfer function. Keep
// this media-only conversion separate; the accepted pack/world shader is intact.
@fragment fn media_unorm_fs(o: Out) -> @location(0) vec4<f32> {
    let texel = textureSample(base_color, base_sampler, clamp(o.uv, vec2<f32>(0.0), vec2<f32>(1.0)));
    let rgb = mix(vec3<f32>(0.025), texel.rgb * o.color, texel.a);
    let encoded = select(12.92 * rgb, 1.055 * pow(rgb, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055), rgb > vec3<f32>(0.0031308));
    return vec4<f32>(encoded, 1.0);
}
// Only world placards/media objects receive fog; viewer and inspector stay clear.
@fragment fn object_fs(o: Out) -> @location(0) vec4<f32> {
    let texel = textureSample(base_color, base_sampler, clamp(o.uv, vec2<f32>(0.0), vec2<f32>(1.0)));
    return vec4<f32>(fogged(mix(vec3<f32>(0.025), texel.rgb * o.color, texel.a), o.world_position), 1.0);
}
@fragment fn object_unorm_fs(o: Out) -> @location(0) vec4<f32> {
    let texel = textureSample(base_color, base_sampler, clamp(o.uv, vec2<f32>(0.0), vec2<f32>(1.0)));
    let rgb = fogged(mix(vec3<f32>(0.025), texel.rgb * o.color, texel.a), o.world_position);
    let encoded = select(12.92 * rgb, 1.055 * pow(rgb, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055), rgb > vec3<f32>(0.0031308));
    return vec4<f32>(encoded, 1.0);
}
