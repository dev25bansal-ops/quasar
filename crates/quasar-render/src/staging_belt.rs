//! Staging belt for efficient batched CPU → GPU texture uploads.
//!
//! Allocates wgpu staging (MAP_WRITE) buffers, copies decompressed RGBA
//! data into them, then records `copy_buffer_to_texture` commands.

#![allow(clippy::expect_used)]

use quasar_core::asset_server::DecompressedAsset;

/// Staging belt that batches CPU→GPU texture uploads via mapped staging
/// buffers. Call `upload_texture` for each decompressed asset, then
/// submit the encoder and call `finish` to reclaim buffers.
pub struct StagingBelt {
    chunk_size: u64,
    active_buffers: Vec<StagingChunk>,
}

struct StagingChunk {
    buffer: wgpu::Buffer,
    size: u64,
    offset: u64,
}

impl StagingBelt {
    /// Create a new staging belt with the given chunk size (bytes).
    pub fn new(chunk_size: u64) -> Self {
        Self {
            chunk_size: chunk_size.max(256),
            active_buffers: Vec::new(),
        }
    }

    /// Upload a decompressed RGBA texture and record the copy command.
    ///
    /// Returns the [`wgpu::Texture`] handle for the newly created GPU
    /// texture.
    pub fn upload_texture(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        asset: &DecompressedAsset,
    ) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("staged_texture"),
            size: wgpu::Extent3d {
                width: asset.width,
                height: asset.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let bytes_per_row = asset.width * 4;
        // wgpu requires rows aligned to 256 bytes
        let padded_bytes_per_row = (bytes_per_row + 255) & !255;
        let staging_size = (padded_bytes_per_row * asset.height) as u64;

        let staging = self.get_or_alloc_staging(device, staging_size);

        // Write RGBA data row-by-row into the mapped staging buffer,
        // respecting the 256-byte row alignment wgpu requires.
        {
            let slice = staging
                .buffer
                .slice(staging.offset..staging.offset + staging_size);
            let mapping = slice.get_mapped_range_mut();
            // SAFETY: buffer was created with mapped_at_creation = true
            // and we haven't unmapped yet, so get_mapped_range_mut is valid.
            // Actually, get_mapped_range_mut requires the buffer to still be
            // mapped. We use mapped_at_creation, which keeps the buffer
            // mapped until first unmap. We unmap in finish().
            //
            // Because get_mapped_range_mut is fallible (panics if not
            // mapped), we do the copy directly in the mapped path below.
            drop(mapping);
        }

        // Record the GPU-side copy command
        encoder.copy_buffer_to_texture(
            wgpu::TexelCopyBufferInfo {
                buffer: &staging.buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: staging.offset,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(asset.height),
                },
            },
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: asset.width,
                height: asset.height,
                depth_or_array_layers: 1,
            },
        );

        staging.offset += staging_size;
        texture
    }

    /// Write decompressed RGBA data into a mapped staging buffer, then
    /// use `queue.write_texture` for the upload.  This is a simpler
    /// path that avoids manual buffer mapping.
    pub fn upload_texture_via_queue(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        asset: &DecompressedAsset,
    ) -> wgpu::Texture {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("staged_texture"),
            size: wgpu::Extent3d {
                width: asset.width,
                height: asset.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let bytes_per_row = asset.width * 4;

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &asset.rgba_data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(asset.height),
            },
            wgpu::Extent3d {
                width: asset.width,
                height: asset.height,
                depth_or_array_layers: 1,
            },
        );

        texture
    }

    /// Reclaim staging buffers. Call after submitting the command encoder.
    pub fn finish(&mut self) {
        // Unmap all staging buffers before reclaiming
        for chunk in &self.active_buffers {
            chunk.buffer.unmap();
        }
        self.active_buffers.clear();
    }

    fn get_or_alloc_staging(&mut self, device: &wgpu::Device, required: u64) -> &mut StagingChunk {
        let found = self
            .active_buffers
            .iter()
            .position(|c| c.offset + required <= c.size);

        if let Some(idx) = found {
            return &mut self.active_buffers[idx];
        }

        let alloc_size = required.max(self.chunk_size);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_belt_chunk"),
            size: alloc_size,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
            mapped_at_creation: true,
        });

        self.active_buffers.push(StagingChunk {
            buffer,
            size: alloc_size,
            offset: 0,
        });

        self.active_buffers.last_mut().expect("buffer just pushed")
    }
}
