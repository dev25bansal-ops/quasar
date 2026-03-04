//! Asset loader — integrates AssetManager with renderer assets.
//!
//! Provides a unified loading interface for textures and meshes using the
//! core `AssetManager`. Assets loaded through this module are tracked
//! with handles that can be used for efficient rendering.

use std::path::Path;

use quasar_core::asset::{Asset, AssetHandle, AssetManager};

use crate::material::Material;
use crate::mesh::{Mesh, MeshData, MeshShape};
use crate::texture::Texture;

pub struct GpuTexture {
    pub texture: Texture,
}

impl Asset for GpuTexture {
    fn asset_type_name() -> &'static str {
        "GpuTexture"
    }
}

pub struct GpuMesh {
    pub mesh: Mesh,
}

impl Asset for GpuMesh {
    fn asset_type_name() -> &'static str {
        "GpuMesh"
    }
}

pub struct GpuMaterial {
    pub material: Material,
}

impl Asset for GpuMaterial {
    fn asset_type_name() -> &'static str {
        "GpuMaterial"
    }
}

pub struct AssetLoader<'a> {
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
    texture_layout: &'a wgpu::BindGroupLayout,
    material_layout: &'a wgpu::BindGroupLayout,
    assets: &'a mut AssetManager,
}

impl<'a> AssetLoader<'a> {
    pub fn new(
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
        texture_layout: &'a wgpu::BindGroupLayout,
        material_layout: &'a wgpu::BindGroupLayout,
        assets: &'a mut AssetManager,
    ) -> Self {
        Self {
            device,
            queue,
            texture_layout,
            material_layout,
            assets,
        }
    }

    pub fn load_texture(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<AssetHandle<GpuTexture>, String> {
        let path = path.as_ref();
        if let Some(handle) = self.assets.handle_for_path::<GpuTexture>(path) {
            return Ok(handle);
        }

        let texture = Texture::from_file(self.device, self.queue, self.texture_layout, path)?;
        let gpu_texture = GpuTexture { texture };
        Ok(self.assets.add_with_path(gpu_texture, path))
    }

    pub fn load_mesh_shape(&mut self, shape: MeshShape) -> AssetHandle<GpuMesh> {
        let key = format!("mesh_shape_{:?}", shape);
        if let Some(handle) = self.assets.handle_for_path::<GpuMesh>(&key) {
            return handle;
        }

        let data = shape.to_mesh_data();
        let mesh = Mesh::from_data(self.device, &data);
        let gpu_mesh = GpuMesh { mesh };
        self.assets.add_with_path(gpu_mesh, key)
    }

    pub fn load_mesh_data(&mut self, data: &MeshData, label: &str) -> AssetHandle<GpuMesh> {
        let mesh = Mesh::from_data(self.device, data);
        let gpu_mesh = GpuMesh { mesh };
        self.assets.add_with_path(gpu_mesh, label)
    }

    pub fn create_material(
        &mut self,
        name: &str,
        base_color: [f32; 4],
        roughness: f32,
        metallic: f32,
    ) -> AssetHandle<GpuMaterial> {
        let mut material = Material::new(self.device, self.material_layout, name);
        material.set_base_color(base_color[0], base_color[1], base_color[2], base_color[3]);
        material.set_roughness(roughness);
        material.set_metallic(metallic);
        material.update(self.queue);
        let gpu_material = GpuMaterial { material };
        self.assets.add(gpu_material)
    }

    pub fn get_texture(&self, handle: &AssetHandle<GpuTexture>) -> Option<&Texture> {
        self.assets.get(handle).map(|gt| &gt.texture)
    }

    pub fn get_mesh(&self, handle: &AssetHandle<GpuMesh>) -> Option<&Mesh> {
        self.assets.get(handle).map(|gm| &gm.mesh)
    }

    pub fn get_material(&self, handle: &AssetHandle<GpuMaterial>) -> Option<&Material> {
        self.assets.get(handle).map(|gm| &gm.material)
    }

    pub fn free_texture(&mut self, handle: &AssetHandle<GpuTexture>) -> bool {
        self.assets.free(handle)
    }

    pub fn free_mesh(&mut self, handle: &AssetHandle<GpuMesh>) -> bool {
        self.assets.free(handle)
    }

    pub fn free_material(&mut self, handle: &AssetHandle<GpuMaterial>) -> bool {
        self.assets.free(handle)
    }

    pub fn texture_count(&self) -> usize {
        self.assets.count::<GpuTexture>()
    }

    pub fn mesh_count(&self) -> usize {
        self.assets.count::<GpuMesh>()
    }

    pub fn material_count(&self) -> usize {
        self.assets.count::<GpuMaterial>()
    }
}

pub struct RenderAssetManager {
    assets: AssetManager,
    texture_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
}

impl RenderAssetManager {
    pub fn new(
        _device: &wgpu::Device,
        texture_layout: wgpu::BindGroupLayout,
        material_layout: wgpu::BindGroupLayout,
    ) -> Self {
        Self {
            assets: AssetManager::new(),
            texture_layout,
            material_layout,
        }
    }

    pub fn loader<'a>(
        &'a mut self,
        device: &'a wgpu::Device,
        queue: &'a wgpu::Queue,
    ) -> AssetLoader<'a> {
        AssetLoader::new(
            device,
            queue,
            &self.texture_layout,
            &self.material_layout,
            &mut self.assets,
        )
    }

    pub fn get_texture(&self, handle: &AssetHandle<GpuTexture>) -> Option<&Texture> {
        self.assets.get(handle).map(|gt| &gt.texture)
    }

    pub fn get_mesh(&self, handle: &AssetHandle<GpuMesh>) -> Option<&Mesh> {
        self.assets.get(handle).map(|gm| &gm.mesh)
    }

    pub fn get_material(&self, handle: &AssetHandle<GpuMaterial>) -> Option<&Material> {
        self.assets.get(handle).map(|gm| &gm.material)
    }

    pub fn free_texture(&mut self, handle: &AssetHandle<GpuTexture>) -> bool {
        self.assets.free(handle)
    }

    pub fn free_mesh(&mut self, handle: &AssetHandle<GpuMesh>) -> bool {
        self.assets.free(handle)
    }

    pub fn free_material(&mut self, handle: &AssetHandle<GpuMaterial>) -> bool {
        self.assets.free(handle)
    }

    pub fn texture_count(&self) -> usize {
        self.assets.count::<GpuTexture>()
    }

    pub fn mesh_count(&self) -> usize {
        self.assets.count::<GpuMesh>()
    }

    pub fn material_count(&self) -> usize {
        self.assets.count::<GpuMaterial>()
    }
}
