struct Camera { view_projection: mat4x4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;
// Host inspection only. The default and all overlays retain source behavior.
struct LabEnvironment {
 model: mat4x4<f32>,
 eye_time: vec4<f32>,
 sun_direction_ambient: vec4<f32>,
 sun_color_intensity: vec4<f32>,
 controls: vec4<f32>,
 fog_color_start: vec4<f32>,
 fog: vec4<f32>,
 viewport_wind: vec4<f32>,
};
@group(0) @binding(1) var<uniform> lab: LabEnvironment;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
struct Output { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) uv2: vec2<f32>, @location(2) diffuse: vec4<f32>, @location(3) world: vec3<f32> };
fn basic_vertex(position: vec3<f32>, uv: vec2<f32>) -> Output {
 var result: Output;
 result.position = camera.view_projection * vec4<f32>(position, 1.0);
 result.world = position;
 result.uv = uv;
 result.uv2 = vec2<f32>(0.0);
 result.diffuse = vec4<f32>(0.0);
 return result;
}
@vertex fn vertex(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> Output {
 return basic_vertex(position, uv);
}
@vertex fn vertex_lab(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> Output {
 var result = basic_vertex(position, uv);
 let world = lab.model * vec4<f32>(position, 1.0);
 result.world = world.xyz;
 result.position = camera.view_projection * world;
 return result;
}
@vertex fn vertex_original(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>, @location(2) uv2: vec2<f32>, @location(3) diffuse: vec4<f32>) -> Output {
 var result: Output;
 let world = lab.model * vec4<f32>(position, 1.0);
 result.world = world.xyz;
 result.position = camera.view_projection * world;
 result.uv = uv; result.uv2 = uv2; result.diffuse = diffuse;
 return result;
}
@fragment fn fragment(input: Output) -> @location(0) vec4<f32> {
 return textureSample(color_texture, color_sampler, input.uv);
}

// Derivative face normals are a host preview. They are oriented towards the
// inspection eye for two-sided meshes; source smooth normals remain unchanged.
fn lab_normal(input: Output) -> vec3<f32> {
 let face = cross(dpdx(input.world), dpdy(input.world));
 var normal = face / max(length(face), 0.000001);
 if dot(normal, lab.eye_time.xyz - input.world) < 0.0 { normal = -normal; }
 return normal;
}
fn lab_color(input: Output, source: vec4<f32>, normal: vec3<f32>) -> vec4<f32> {
 if lab.controls.x == 0.0 { return source; }
 if lab.controls.w == 2.0 { return vec4<f32>(normal * 0.5 + vec3<f32>(0.5), source.a); }
 if lab.controls.w == 3.0 { return vec4<f32>(fract(input.uv), 0.25, source.a); }
 var rgb = source.rgb;
 if lab.controls.w == 1.0 {
   let direct = max(dot(normal, lab.sun_direction_ambient.xyz), 0.0);
   rgb *= vec3<f32>(lab.sun_direction_ambient.w) + lab.sun_color_intensity.rgb * (lab.sun_color_intensity.w * direct);
 }
 let luminance = dot(rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
 rgb = mix(vec3<f32>(luminance), rgb, lab.controls.z) * lab.controls.y;
 if lab.fog.y != 0.0 {
   let distance = length(input.world - lab.eye_time.xyz);
   let amount = clamp((distance - lab.fog_color_start.w) / (lab.fog.x - lab.fog_color_start.w), 0.0, 1.0);
   rgb = mix(rgb, lab.fog_color_start.rgb, amount);
 }
 return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), source.a);
}
@fragment fn fragment_lab(input: Output) -> @location(0) vec4<f32> {
 let normal = lab_normal(input);
 return lab_color(input, textureSample(color_texture, color_sampler, input.uv), normal);
}
// Host inspection cutoff, not reconstructed original alpha-test behavior.
@fragment fn fragment_inspection_cutout(input: Output) -> @location(0) vec4<f32> {
 let normal = lab_normal(input);
 let color = textureSample(color_texture, color_sampler, input.uv);
 if color.a < 0.5 { discard; }
 return lab_color(input, color, normal);
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
 let normal = lab_normal(input);
 return lab_color(input, original_color(input), normal);
}

// An sRGB attachment encodes shader RGB on write. Decode the original code
// values here so opaque Original draws retain their source color bytes.
@fragment fn fragment_original_srgb(input: Output) -> @location(0) vec4<f32> {
 let normal = lab_normal(input);
 let color = lab_color(input, original_color(input), normal);
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


// Procedural screen-space precipitation is a host visual preview only. It has
// no collision, world particle lifetime, audio, or native weather state.
@vertex fn vertex_weather(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
 let x = f32((index << 1u) & 2u);
 let y = f32(index & 2u);
 return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}
fn weather_hash(cell: vec2<f32>) -> f32 {
 return fract(sin(dot(cell, vec2<f32>(127.1, 311.7))) * 43758.5453);
}
@fragment fn fragment_weather(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
 let snow = lab.fog.z == 2.0;
 let time = lab.eye_time.w;
 var alpha = 0.0;
 // Two deterministic layers keep the preview inexpensive and reproducible.
 for (var layer = 0u; layer < 2u; layer += 1u) {
   let scale = 1.0 + f32(layer) * 0.6;
   let size = select(vec2<f32>(32.0, 54.0), vec2<f32>(27.0), snow) * scale;
   let speed = select(240.0, 24.0, snow) / scale;
   let drift = lab.viewport_wind.z * time * 18.0;
   let p = (position.xy + vec2<f32>(drift, -time * speed)) / size;
   let cell = floor(p);
   let seed = weather_hash(cell + vec2<f32>(f32(layer) * 73.0));
   let local = fract(p) - vec2<f32>(0.5);
   let center = (vec2<f32>(seed, weather_hash(cell + vec2<f32>(19.0))) - vec2<f32>(0.5)) * 0.4;
   let q = (local - center) * size;
   var coverage = 0.0;
   if snow {
     let sway = sin(time * 0.8 + seed * 6.283) * 3.0;
     coverage = 1.0 - smoothstep(1.0 * scale, 2.7 * scale, length(q - vec2<f32>(sway, 0.0)));
   } else {
     let slant = q.x + q.y * lab.viewport_wind.z * 0.25;
     coverage = (1.0 - smoothstep(0.3, 1.0, abs(slant))) * (1.0 - smoothstep(5.0, 13.0, abs(q.y)));
   }
   if seed > lab.fog.w { coverage = 0.0; }
   alpha = max(alpha, coverage * select(0.45, 0.8, snow));
 }
 var color = select(vec3<f32>(0.65, 0.78, 0.9), vec3<f32>(0.95, 0.97, 1.0), snow);
 // Match code values on both sRGB presentation and normalized capture targets.
 if lab.viewport_wind.w != 0.0 {
   color = select(color / 12.92, pow((color + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), color > vec3<f32>(0.04045));
 }
 return vec4<f32>(color, alpha);
}
