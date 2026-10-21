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
