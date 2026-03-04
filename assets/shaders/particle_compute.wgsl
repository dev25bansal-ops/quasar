// Quasar Engine — Particle compute shader.
//
// Simulates particles on GPU using compute shaders.

struct ParticleUniforms {
    delta_time: f32,
    gravity: vec3<f32>,
    alive_count: u32,
    _pad: vec3<u32>,
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

@group(0) @binding(0) var<storage, read> particles_in: array<Particle>;
@group(0) @binding(1) var<storage, read_write> particles_out: array<Particle>;
@group(0) @binding(2) var<uniform> uniforms: ParticleUniforms;

@compute @workgroup_size(256)
fn cs_main(@builtin(global_invocation_id) id: vec3<u32>) {
    let index = id.x;
    if (index >= uniforms.alive_count) {
        return;
    }

    var particle = particles_in[index];

    particle.age += uniforms.delta_time;

    if (particle.age >= particle.lifetime) {
        return;
    }

    particle.velocity = particle.velocity + uniforms.gravity * uniforms.delta_time;
    particle.position = particle.position + particle.velocity * uniforms.delta_time;

    let t = particle.age / particle.lifetime;
    particle.color.a = particle.color.a * (1.0 - t);

    particles_out[index] = particle;
}
