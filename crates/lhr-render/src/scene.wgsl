struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vertex(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> Output {
 var result: Output;
 result.position = camera.view_projection * vec4<f32>(position, 1.0);
 result.uv = uv;
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
// Original material decisions only; legacy inspection and menu shaders above
// keep their separately selected presentation policy.
@fragment fn fragment_original(input: Output) -> @location(0) vec4<f32> {
 let color = textureSample(color_texture, color_sampler, input.uv) * material.color;
 if color.a < material.cutoff.x { discard; }
 return color;
}
