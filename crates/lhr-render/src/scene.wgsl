struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) uv2: vec2<f32>, @location(2) diffuse: vec4<f32> };
@vertex fn vertex(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> Output {
 var result: Output;
 result.position = camera.view_projection * vec4<f32>(position, 1.0);
 result.uv = uv;
 result.uv2 = vec2<f32>(0.0);
 result.diffuse = vec4<f32>(0.0);
 return result;
}
@vertex fn vertex_original(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) uv2: vec2<f32>, @location(3) diffuse: vec4<f32>) -> Output {
 var result: Output;
 result.position = camera.view_projection * vec4<f32>(position, 1.0);
 result.uv = uv; result.uv2 = uv2; result.diffuse = diffuse;
 return result;
}
@fragment fn fragment(input: Output) -> @location(0) vec4<f32> {
 return textureSample(color_texture, color_sampler, input.uv);
}

// Host inspection cutoff, not reconstructed original alpha-test behavior.
@fragment fn fragment_inspection_cutout(input: Output) -> @location(0) vec4<f32> {
 let color = textureSample(color_texture, color_sampler, input.uv);
 if color.a < 0.5 { discard; }
 return color;
}

struct MaterialParameters { color: vec4<f32>, cutoff: vec4<f32> };
@group(2) @binding(0) var<uniform> material: MaterialParameters;
@group(3) @binding(0) var lighting_texture: texture_2d<f32>;
@group(3) @binding(1) var lighting_sampler: sampler;
// Original material decisions only; legacy inspection and menu shaders above
// keep their separately selected presentation policy.
fn original_color(input: Output) -> vec4<f32> {
 let base = textureSample(color_texture, color_sampler, input.uv);
 var color = base * material.color;
 if material.cutoff.y == 1.0 {
   let illumination = clamp(textureSample(lighting_texture, lighting_sampler, input.uv2).rgb + input.diffuse.rgb, vec3<f32>(0.0), vec3<f32>(1.0));
   var rgb = illumination;
   if material.cutoff.w != 0.0 { rgb = illumination * base.rgb * material.cutoff.z; }
   color = vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), color.a);
 } else if material.cutoff.y == 2.0 {
   color = vec4<f32>(clamp(base.rgb * input.diffuse.rgb * material.cutoff.z, vec3<f32>(0.0), vec3<f32>(1.0)), color.a);
 }
 if color.a < material.cutoff.x { discard; }
 return color;
}

@fragment fn fragment_original(input: Output) -> @location(0) vec4<f32> {
 return original_color(input);
}

// An sRGB attachment encodes shader RGB on write. Decode the original code
// values here so opaque Original draws retain their source color bytes.
@fragment fn fragment_original_srgb(input: Output) -> @location(0) vec4<f32> {
 let color = original_color(input);
 let rgb = select(color.rgb / 12.92, pow((color.rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), color.rgb > vec3<f32>(0.04045));
 return vec4<f32>(rgb, color.a);
}

// Legacy menu canvases are authored in sRGB. Canonical D3D8-like scene targets
// store normalized RGB without framebuffer encoding, so restore canvas bytes
// before alpha compositing onto that target.
@fragment fn fragment_overlay_unorm(input: Output) -> @location(0) vec4<f32> {
 let color = textureSample(color_texture, color_sampler, input.uv);
 let encoded = select(color.rgb * 12.92, 1.055 * pow(color.rgb, vec3<f32>(1.0 / 2.4)) - 0.055, color.rgb > vec3<f32>(0.0031308));
 return vec4<f32>(encoded, color.a);
}
