// Quasar Engine — Particle render shader.
//
// Renders particles as billboards facing the camera.

struct CameraUniform {
    view_proj: mat4x4<f32>,
    view_position: vec3<f32>,
    _pad: f32,
};

struct Particle {
    position: vec3<f32>,
    velocity: vec3<f32>,
    color: vec4<f32>,
    scale: f32,
    lifetime: f32,
    age: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(0) @binding(1) var<storage, read> particles: array<Particle>;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @builtin(vertex_index) vertex_index: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    let particle_index = in.vertex_index / 6u;
    let particle = particles[particle_index];

    let right = normalize(cross(camera.view_position - particle.position, vec3<f32>(0.0, 1.0, 0.0)));
    let up = normalize(cross(right, camera.view_position - particle.position));

    let world_pos = particle.position +
        (right * in.position.x * particle.scale) +
        (up * in.position.y * particle.scale);

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.color = particle.color;
    out.uv = in.position + vec2<f32>(0.5);

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let dist = length(in.uv - vec2<f32>(0.5));
    if (dist > 0.5) {
        discard;
    }

    let alpha = 1.0 - smoothstep(0.3, 0.5, dist);
    return vec4<f32>(in.color.rgb, in.color.a * alpha);
}
