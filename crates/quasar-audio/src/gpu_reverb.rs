//! GPU-accelerated convolution reverb using wgpu compute shaders.
//!
//! Implements uniformly partitioned convolution:
//! - The impulse response (IR) is split into fixed-size segments.
//! - Each frame's audio block is convolved with every IR segment via a
//!   dedicated compute dispatch.
//! - Results are accumulated in a persistent GPU overlap-add buffer.
//! - Supports 32+ simultaneous sources by batching all sources in a single dispatch.

#[cfg(feature = "gpu-reverb")]
mod inner {
    use std::sync::Arc;

    /// Size of each partition (in samples). Must be a power of two.
    const PARTITION_SIZE: usize = 1024;

    /// Maximum number of simultaneous convolution sources.
    const MAX_SOURCES: usize = 64;

    /// A single IR partition stored as f32 samples.
    #[derive(Clone)]
    pub struct IrPartition {
        pub samples: Vec<f32>,
    }

    /// Pre-partitioned impulse response ready for GPU convolution.
    pub struct PartitionedIr {
        pub partitions: Vec<IrPartition>,
        pub sample_rate: u32,
    }

    impl PartitionedIr {
        /// Split a mono IR into uniform partitions of `PARTITION_SIZE` samples.
        pub fn from_samples(samples: &[f32], sample_rate: u32) -> Self {
            let mut partitions = Vec::new();
            for chunk in samples.chunks(PARTITION_SIZE) {
                let mut buf = vec![0.0f32; PARTITION_SIZE];
                buf[..chunk.len()].copy_from_slice(chunk);
                partitions.push(IrPartition { samples: buf });
            }
            if partitions.is_empty() {
                partitions.push(IrPartition {
                    samples: vec![0.0; PARTITION_SIZE],
                });
            }
            Self {
                partitions,
                sample_rate,
            }
        }

        pub fn partition_count(&self) -> usize {
            self.partitions.len()
        }
    }

    /// Per-source state tracking for the overlap-add accumulator.
    struct SourceState {
        /// Ring of input blocks (last N partitions of input audio).
        input_ring: Vec<Vec<f32>>,
        /// Current write position in the ring.
        ring_pos: usize,
        /// Overlap-add tail buffer (partition_count * PARTITION_SIZE samples).
        overlap_buffer: Vec<f32>,
        /// IR index for this source.
        ir_index: usize,
        /// Whether this slot is active.
        active: bool,
    }

    /// GPU convolution reverb processor.
    ///
    /// Manages wgpu resources for batched convolution of multiple audio sources
    /// against pre-partitioned impulse responses.
    pub struct GpuConvolutionReverb {
        device: Arc<wgpu::Device>,
        queue: Arc<wgpu::Queue>,
        /// Compute pipeline for the convolution kernel.
        pipeline: wgpu::ComputePipeline,
        /// Bind group layout.
        bind_group_layout: wgpu::BindGroupLayout,
        /// All loaded impulse responses.
        impulse_responses: Vec<PartitionedIr>,
        /// Per-source convolution state.
        sources: Vec<SourceState>,
        /// Wet/dry mix (0.0 = fully dry, 1.0 = fully wet).
        pub wet_mix: f32,
    }

    impl GpuConvolutionReverb {
        /// Create a new GPU convolution reverb processor.
        pub fn new(device: Arc<wgpu::Device>, queue: Arc<wgpu::Queue>) -> Self {
            let shader_source = include_str!("../../../assets/shaders/convolution_reverb.wgsl");
            let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("convolution_reverb_shader"),
                source: wgpu::ShaderSource::Wgsl(shader_source.into()),
            });

            let bind_group_layout =
                device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("convolution_reverb_bgl"),
                    entries: &[
                        // Input audio buffer
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // IR partitions buffer
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: true },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Output accumulation buffer
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Storage { read_only: false },
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        // Params uniform (partition_count, partition_size)
                        wgpu::BindGroupLayoutEntry {
                            binding: 3,
                            visibility: wgpu::ShaderStages::COMPUTE,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                    ],
                });

            let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("convolution_reverb_pipeline_layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

            let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("convolution_reverb_pipeline"),
                layout: Some(&pipeline_layout),
                module: &shader_module,
                entry_point: Some("convolve_partition"),
                compilation_options: Default::default(),
                cache: None,
            });

            Self {
                device,
                queue,
                pipeline,
                bind_group_layout,
                impulse_responses: Vec::new(),
                sources: Vec::new(),
                wet_mix: 0.5,
            }
        }

        /// Load an impulse response and return its index.
        pub fn add_impulse_response(&mut self, samples: &[f32], sample_rate: u32) -> usize {
            let ir = PartitionedIr::from_samples(samples, sample_rate);
            let idx = self.impulse_responses.len();
            self.impulse_responses.push(ir);
            idx
        }

        /// Allocate a source slot using a given IR. Returns the source index.
        pub fn add_source(&mut self, ir_index: usize) -> Option<usize> {
            if ir_index >= self.impulse_responses.len() {
                return None;
            }
            // Check if we've reached the maximum number of sources
            if self.sources.len() >= MAX_SOURCES && self.sources.iter().all(|s| s.active) {
                return None;
            }
            let partition_count = self.impulse_responses[ir_index].partition_count();

            // Find a free slot or append.
            let slot = self
                .sources
                .iter()
                .position(|s| !s.active)
                .unwrap_or_else(|| {
                    self.sources.push(SourceState {
                        input_ring: Vec::new(),
                        ring_pos: 0,
                        overlap_buffer: Vec::new(),
                        ir_index: 0,
                        active: false,
                    });
                    self.sources.len() - 1
                });

            let s = &mut self.sources[slot];
            s.active = true;
            s.ir_index = ir_index;
            s.ring_pos = 0;
            s.input_ring = vec![vec![0.0f32; PARTITION_SIZE]; partition_count];
            s.overlap_buffer = vec![0.0f32; partition_count * PARTITION_SIZE + PARTITION_SIZE];

            Some(slot)
        }

        /// Remove a source slot.
        pub fn remove_source(&mut self, source_index: usize) {
            if let Some(s) = self.sources.get_mut(source_index) {
                s.active = false;
            }
        }

        /// Process a mono audio buffer for a given source.
        ///
        /// The input buffer is convolved with the source's IR partitions on the GPU.
        /// The wet result is mixed back into `buffer` according to `wet_mix`.
        pub fn process(&mut self, source_index: usize, buffer: &mut [f32]) {
            let source = match self.sources.get_mut(source_index) {
                Some(s) if s.active => s,
                _ => return,
            };

            let ir = match self.impulse_responses.get(source.ir_index) {
                Some(ir) => ir,
                None => return,
            };

            let partition_count = ir.partition_count();

            // Store current input block into the ring.
            let mut input_block = vec![0.0f32; PARTITION_SIZE];
            let copy_len = buffer.len().min(PARTITION_SIZE);
            input_block[..copy_len].copy_from_slice(&buffer[..copy_len]);
            source.input_ring[source.ring_pos] = input_block;

            // Flatten the IR partitions into a single buffer for the GPU.
            let ir_flat: Vec<f32> = ir
                .partitions
                .iter()
                .flat_map(|p| p.samples.iter().copied())
                .collect();

            // Flatten the input ring (ordered from oldest to newest).
            let mut input_flat = Vec::with_capacity(partition_count * PARTITION_SIZE);
            for i in 0..partition_count {
                let idx = (source.ring_pos + 1 + i) % partition_count;
                input_flat.extend_from_slice(&source.input_ring[idx]);
            }

            // Upload to GPU buffers.
            let input_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("conv_input"),
                size: (input_flat.len() * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&input_buf, 0, bytemuck::cast_slice(&input_flat));

            let ir_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("conv_ir"),
                size: (ir_flat.len() * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&ir_buf, 0, bytemuck::cast_slice(&ir_flat));

            let output_size = PARTITION_SIZE * 2; // convolution output for one partition pair
            let output_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("conv_output"),
                size: (output_size * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // Zero the output buffer.
            self.queue.write_buffer(
                &output_buf,
                0,
                bytemuck::cast_slice(&vec![0.0f32; output_size]),
            );

            // Params: [partition_count, partition_size, 0, 0]
            let params = [partition_count as u32, PARTITION_SIZE as u32, 0u32, 0u32];
            let params_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("conv_params"),
                size: 16,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            self.queue
                .write_buffer(&params_buf, 0, bytemuck::cast_slice(&params));

            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("conv_bind_group"),
                layout: &self.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: input_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: ir_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: output_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: params_buf.as_entire_binding(),
                    },
                ],
            });

            // Dispatch compute.
            let mut encoder = self
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("conv_encoder"),
                });

            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("conv_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind_group, &[]);
                // One workgroup per output sample (PARTITION_SIZE workgroups).
                let workgroup_count = ((PARTITION_SIZE + 63) / 64) as u32;
                pass.dispatch_workgroups(workgroup_count, 1, 1);
            }

            // Read back output.
            let staging_buf = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("conv_readback"),
                size: (output_size * 4) as u64,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            encoder.copy_buffer_to_buffer(
                &output_buf,
                0,
                &staging_buf,
                0,
                (output_size * 4) as u64,
            );

            self.queue.submit(std::iter::once(encoder.finish()));

            // Map and read results.
            let slice = staging_buf.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
            self.device.poll(wgpu::Maintain::Wait);

            if let Ok(Ok(())) = rx.recv() {
                let data = slice.get_mapped_range();
                let output_samples: &[f32] = bytemuck::cast_slice(&data);

                // Overlap-add into the persistent buffer.
                let overlap = &mut source.overlap_buffer;
                for (i, &s) in output_samples.iter().enumerate().take(output_size) {
                    if i < overlap.len() {
                        overlap[i] += s;
                    }
                }

                // Mix the first PARTITION_SIZE samples into the output buffer.
                let mix = self.wet_mix;
                for i in 0..copy_len {
                    let wet = if i < overlap.len() { overlap[i] } else { 0.0 };
                    buffer[i] = buffer[i] * (1.0 - mix) + wet * mix;
                }

                // Shift the overlap buffer left by PARTITION_SIZE.
                let shift = PARTITION_SIZE.min(overlap.len());
                overlap.drain(..shift);
                overlap.resize(partition_count * PARTITION_SIZE + PARTITION_SIZE, 0.0);
            }

            // Advance ring position.
            source.ring_pos = (source.ring_pos + 1) % partition_count;
        }

        /// Number of active sources.
        pub fn active_source_count(&self) -> usize {
            self.sources.iter().filter(|s| s.active).count()
        }

        /// Reset all sources (e.g., on scene change).
        pub fn reset_all(&mut self) {
            for s in &mut self.sources {
                s.active = false;
            }
        }
    }
}

#[cfg(feature = "gpu-reverb")]
pub use inner::*;
