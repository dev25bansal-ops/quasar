//! Platform-abstracted file I/O for cross-platform asset loading.
//!
//! Handles platform-specific file access:
//! - Desktop: Standard filesystem via tokio
//! - WASM: Browser Fetch API
//! - Android: AssetManager via JNI (registered by quasar-mobile)

use std::path::Path;
use std::sync::OnceLock;

/// Trait for platform-specific asset loading.
pub trait AssetLoader: Send + Sync {
    /// Read an asset file asynchronously.
    fn read_asset(&self, path: &Path) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send;
    
    /// Check if an asset exists.
    fn asset_exists(&self, path: &Path) -> bool;
    
    /// List assets in a directory.
    fn list_assets(&self, path: &Path) -> Result<Vec<String>, String>;
}

static ASSET_LOADER: OnceLock<Box<dyn AssetLoader>> = OnceLock::new();

/// Register a custom asset loader (used by quasar-mobile for Android).
pub fn register_asset_loader(loader: Box<dyn AssetLoader>) -> Result<(), &'static str> {
    ASSET_LOADER.set(loader).map_err(|_| "Asset loader already registered")
}

/// Get the registered asset loader.
pub fn asset_loader() -> Option<&'static dyn AssetLoader> {
    ASSET_LOADER.get().map(|b| b.as_ref())
}

/// Platform-abstracted async file read.
pub async fn read_file(path: &Path) -> Result<Vec<u8>, FileError> {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        read_file_desktop(path).await
    }
    
    #[cfg(target_arch = "wasm32")]
    {
        read_file_wasm(path).await
    }
    
    #[cfg(target_os = "android")]
    {
        read_file_android(path).await
    }
}

/// Desktop implementation using tokio.
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
async fn read_file_desktop(path: &Path) -> Result<Vec<u8>, FileError> {
    tokio::fs::read(path)
        .await
        .map_err(|e| FileError::Io(e.to_string()))
}

/// WASM implementation using Fetch API.
#[cfg(target_arch = "wasm32")]
async fn read_file_wasm(path: &Path) -> Result<Vec<u8>, FileError> {
    let url = format!("/assets/{}", path.display());
    
    wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .ok_or_else(|| FileError::Platform("No window object".into()))?
            .fetch_with_str(&url)
    )
    .await
    .map_err(|e| FileError::Platform(format!("{:?}", e)))?;
    
    Ok(vec![])
}

/// Android implementation using registered AssetLoader.
#[cfg(target_os = "android")]
async fn read_file_android(path: &Path) -> Result<Vec<u8>, FileError> {
    asset_loader()
        .ok_or_else(|| FileError::Platform("Asset loader not registered".into()))?
        .read_asset(path)
        .await
        .map_err(FileError::Platform)
}

/// File error types.
#[derive(Debug, Clone)]
pub enum FileError {
    Io(String),
    Platform(String),
    NotFound(String),
    InvalidPath(String),
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Platform(e) => write!(f, "Platform error: {}", e),
            Self::NotFound(p) => write!(f, "File not found: {}", p),
            Self::InvalidPath(p) => write!(f, "Invalid path: {}", p),
        }
    }
}

impl std::error::Error for FileError {}

/// Check if a file exists.
pub fn file_exists(path: &Path) -> bool {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        path.exists()
    }
    
    #[cfg(target_arch = "wasm32")]
    {
        true
    }
    
    #[cfg(target_os = "android")]
    {
        asset_loader()
            .map(|loader| loader.asset_exists(path))
            .unwrap_or(false)
    }
}

/// Get the base assets path for the current platform.
pub fn assets_base_path() -> std::path::PathBuf {
    #[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
    {
        std::path::PathBuf::from("assets")
    }
    
    #[cfg(target_arch = "wasm32")]
    {
        std::path::PathBuf::from("")
    }
    
    #[cfg(target_os = "android")]
    {
        std::path::PathBuf::from("")
    }
}

#[cfg(target_arch = "wasm32")]
pub mod wasm {
    use wasm_bindgen::prelude::*;
    use web_sys::{window, Response};
    
    /// Fetch bytes from URL.
    pub async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
        let resp = window()
            .ok_or("No window")?
            .fetch_with_str(url);
        
        let resp = wasm_bindgen_futures::JsFuture::from(resp)
            .await
            .map_err(|e| format!("{:?}", e))?;
        
        let resp: Response = resp.into();
        
        let array_buffer = wasm_bindgen_futures::JsFuture::from(
            resp.array_buffer()
                .map_err(|e| format!("{:?}", e))?
        )
        .await
        .map_err(|e| format!("{:?}", e))?;
        
        let uint8_array = js_sys::Uint8Array::new(&array_buffer);
        Ok(uint8_array.to_vec())
    }
}

#[cfg(target_os = "android")]
pub mod android {
    use std::path::Path;
    
    /// Handle Android lifecycle event.
    pub fn on_pause() {
        log::debug!("Android onPause");
    }
    
    pub fn on_resume() {
        log::debug!("Android onResume");
    }
    
    pub fn on_trim_memory(level: i32) {
        log::debug!("Android trim memory level: {}", level);
    }
    
    /// List assets in a directory using the registered loader.
    pub fn list_assets(path: &Path) -> Result<Vec<String>, String> {
        crate::platform::asset_loader()
            .ok_or("Asset loader not registered")?
            .list_assets(path)
    }
}
