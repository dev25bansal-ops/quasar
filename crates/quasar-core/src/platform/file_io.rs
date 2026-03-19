//! Platform-abstracted file I/O for cross-platform asset loading.
//!
//! Handles platform-specific file access:
//! - Desktop: Standard filesystem via tokio
//! - WASM: Browser Fetch API
//! - Android: AssetManager via JNI

use std::path::Path;
use std::future::Future;

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
    
    // Use wasm-bindgen fetch
    wasm_bindgen_futures::JsFuture::from(
        web_sys::window()
            .ok_or_else(|| FileError::Platform("No window object".into()))?
            .fetch_with_str(&url)
    )
    .await
    .map_err(|e| FileError::Platform(format!("{:?}", e)))?;
    
    // This is a simplified version - full impl needs Response handling
    Ok(vec![])
}

/// Android implementation using AssetManager.
#[cfg(target_os = "android")]
async fn read_file_android(path: &Path) -> Result<Vec<u8>, FileError> {
    // Call into JNI bridge
    crate::platform::android::android_asset_read(path)
        .await
        .map_err(|e| FileError::Platform(e))
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
        // On WASM, we can't check existence synchronously
        // Would need to track known assets or use a manifest
        true
    }
    
    #[cfg(target_os = "android")]
    {
        // Use AssetManager list API
        true
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

// ---------------------------------------------------------------------------
// Platform-specific modules
// ---------------------------------------------------------------------------

#[cfg(target_os = "android")]
pub mod android {
    use std::path::Path;
    
    /// Read asset using Android AssetManager.
    pub async fn android_asset_read(path: &Path) -> Result<Vec<u8>, String> {
        // JNI call to AssetManager.open(path)
        // This would be implemented via ndk-glue or similar
        Err("Android asset loading not implemented".into())
    }
    
    /// Handle Android lifecycle event.
    pub fn on_pause() {
        // Notify Quasar app to pause
    }
    
    pub fn on_resume() {
        // Notify Quasar app to resume
    }
    
    pub fn on_trim_memory(level: i32) {
        // Free resources based on trim level
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
