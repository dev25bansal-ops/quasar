// Convolution reverb compute shader — uniformly partitioned convolution.
//
// Each invocation computes one output sample by accumulating the dot-product
// contributions from all IR partitions against the corresponding input ring
// segments.

struct Params {
    partition_count: u32,
    partition_size: u32,
    _pad0: u32,
    _pad1: u32,
}

@group(0) @binding(0) var<storage, read> input_ring: array<f32>;
@group(0) @binding(1) var<storage, read> ir_partitions: array<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;

@compute @workgroup_size(64)
fn convolve_partition(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out_idx = gid.x;
    let ps = params.partition_size;

    if out_idx >= ps {
        return;
    }

    var acc: f32 = 0.0;

    // For each IR partition, multiply-accumulate with the corresponding
    // input block segment using time-domain convolution per partition.
    for (var p: u32 = 0u; p < params.partition_count; p = p + 1u) {
        let ir_base = p * ps;
        let input_base = (params.partition_count - 1u - p) * ps;

        // Standard time-domain convolution within this partition:
        // output[out_idx] += sum over k of input[input_base + out_idx - k] * ir[ir_base + k]
        // where k ranges such that input index is valid.
        for (var k: u32 = 0u; k <= out_idx; k = k + 1u) {
            let input_idx = input_base + out_idx - k;
            let ir_idx = ir_base + k;
            acc += input_ring[input_idx] * ir_partitions[ir_idx];
        }
    }

    output[out_idx] = acc;
}
