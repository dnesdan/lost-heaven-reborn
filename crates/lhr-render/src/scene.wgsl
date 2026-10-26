struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec3<f32>, @location(2) lightmap_uv: vec2<f32> };
@vertex fn vertex(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec3<f32>, @location(3) lightmap_uv: vec2<f32>) -> Output {
 var result: Output;
 result.position = camera.view_projection * vec4<f32>(position, 1.0);
 result.uv = uv;
 result.color = color;
 result.lightmap_uv = lightmap_uv;
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
@group(3) @binding(0) var lightmap_texture: texture_2d<f32>;
@group(3) @binding(1) var lightmap_sampler: sampler;
fn srgb_to_linear(color: vec3<f32>) -> vec3<f32> {
 return select(pow((color + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), color / 12.92, color <= vec3<f32>(0.04045));
}
// Original material decisions only; legacy inspection and menu shaders above
// keep their separately selected presentation policy.
@fragment fn fragment_original(input: Output) -> @location(0) vec4<f32> {
 let sampled = textureSample(color_texture, color_sampler, input.uv);
 var color = sampled * material.color;
 if material.cutoff.y > 0.5 {
  var diffuse = input.color;
  var alpha = sampled.a;
  if material.cutoff.y > 1.5 {
   // Source single-pass mode: stage 0 ADD saturates before stage 1 MODULATE2X.
   let lightmap = textureSample(lightmap_texture, lightmap_sampler, input.lightmap_uv);
   diffuse = clamp(lightmap.rgb + input.color, vec3<f32>(0.0), vec3<f32>(1.0));
   alpha = lightmap.a * sampled.a;
  }
  var code_rgb = clamp(2.0 * sampled.rgb * diffuse, vec3<f32>(0.0), vec3<f32>(1.0));
  // Compensate only an sRGB framebuffer's automatic encoding. Raw UNORM
  // readbacks and sRGB windows must produce the same source code-space bytes.
  if material.cutoff.z > 0.5 { code_rgb = srgb_to_linear(code_rgb); }
  color = vec4<f32>(code_rgb, alpha);
 }
 if color.a < material.cutoff.x { discard; }
 return color;
}
