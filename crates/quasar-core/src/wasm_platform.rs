//! WebAssembly platform support.
//!
//! Provides WASM-specific utilities and abstractions:
//! - Memory management for WASM
//! - File system abstraction (virtual FS)
//! - Async runtime support
//! - WebGL rendering integration
//!
//! # Example
//!
//! ```ignore
//! #[cfg(target_arch = "wasm32")]
//! use quasar_core::wasm_platform::*;
//!
//! // Initialize WASM platform
//! init_wasm_platform();
//!
//! // Use virtual file system
//! let mut vfs = VirtualFileSystem::new();
//! vfs.mount("/assets", include_bytes!("../assets/"));
//! ```

#[cfg(target_arch = "wasm32")]
pub mod wasm32 {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    pub fn init_panic_hook() {
        #[cfg(feature = "console_error_panic_hook")]
        console_error_panic_hook::set_once();
    }

    pub fn log(message: &str) {
        #[cfg(feature = "console_log")]
        web_sys::console::log_1(&message.into());
    }

    pub fn log_error(message: &str) {
        #[cfg(feature = "console_log")]
        web_sys::console::error_1(&message.into());
    }

    pub fn log_warn(message: &str) {
        #[cfg(feature = "console_log")]
        web_sys::console::warn_1(&message.into());
    }

    pub struct VirtualFileSystem {
        mounts: HashMap<String, Vec<u8>>,
    }

    impl VirtualFileSystem {
        pub fn new() -> Self {
            Self {
                mounts: HashMap::new(),
            }
        }

        pub fn mount(&mut self, path: &str, data: &'static [u8]) {
            self.mounts.insert(path.to_string(), data.to_vec());
        }

        pub fn mount_vec(&mut self, path: &str, data: Vec<u8>) {
            self.mounts.insert(path.to_string(), data);
        }

        pub fn read(&self, path: &str) -> Option<&[u8]> {
            self.mounts.get(path).map(|v| v.as_slice())
        }

        pub fn read_string(&self, path: &str) -> Option<String> {
            self.read(path)
                .and_then(|bytes| String::from_utf8(bytes.to_vec()).ok())
        }

        pub fn exists(&self, path: &str) -> bool {
            self.mounts.contains_key(path)
        }

        pub fn list(&self) -> Vec<&str> {
            self.mounts.keys().map(|s| s.as_str()).collect()
        }

        pub fn unmount(&mut self, path: &str) -> Option<Vec<u8>> {
            self.mounts.remove(path)
        }
    }

    impl Default for VirtualFileSystem {
        fn default() -> Self {
            Self::new()
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MemoryStats {
        pub total_memory: u32,
        pub used_memory: u32,
        pub available_memory: u32,
    }

    pub fn get_memory_stats() -> MemoryStats {
        use wasm_bindgen::JsCast;
        use web_sys::wasm_bindgen::memory;

        let memory = memory();
        let buffer = memory.dyn_into::<js_sys::WebAssembly::Memory>().ok();

        if let Some(mem) = buffer {
            MemoryStats {
                total_memory: mem.buffer().byte_length() as u32,
                used_memory: 0,
                available_memory: 0,
            }
        } else {
            MemoryStats {
                total_memory: 0,
                used_memory: 0,
                available_memory: 0,
            }
        }
    }

    pub fn now_ms() -> f64 {
        use web_sys::window;
        window()
            .map(|w| w.performance().map(|p| p.now()).unwrap_or(0.0))
            .unwrap_or(0.0)
    }

    pub fn now_secs() -> f64 {
        now_ms() / 1000.0
    }

    pub struct LocalStorage {
        storage: web_sys::Storage,
    }

    impl LocalStorage {
        pub fn new() -> Option<Self> {
            web_sys::window()
                .and_then(|w| w.local_storage().ok())
                .flatten()
                .map(|storage| Self { storage })
        }

        pub fn get(&self, key: &str) -> Option<String> {
            self.storage.get_item(key).ok().flatten()
        }

        pub fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.storage
                .set_item(key, value)
                .map_err(|e| format!("Failed to set localStorage item: {:?}", e))
        }

        pub fn remove(&self, key: &str) {
            self.storage.remove_item(key).ok();
        }

        pub fn clear(&self) {
            self.storage.clear().ok();
        }

        pub fn keys(&self) -> Vec<String> {
            let mut keys = Vec::new();
            let length = self.storage.length().unwrap_or(0);
            for i in 0..length {
                if let Some(key) = self.storage.key(i).ok().flatten() {
                    keys.push(key);
                }
            }
            keys
        }
    }

    impl Default for LocalStorage {
        fn default() -> Self {
            Self::new().expect("localStorage not available")
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WebGlVersion {
        WebGl1,
        WebGl2,
    }

    pub fn detect_webgl_version() -> Option<WebGlVersion> {
        use web_sys::window;

        let window = window()?;
        let document = window.document()?;
        let canvas = document.create_element("canvas").ok()?;

        let canvas: web_sys::HtmlCanvasElement = canvas.dyn_into().ok()?;

        if canvas.get_context("webgl2").ok().flatten().is_some() {
            Some(WebGlVersion::WebGl2)
        } else if canvas.get_context("webgl").ok().flatten().is_some() {
            Some(WebGlVersion::WebGl1)
        } else {
            None
        }
    }

    use wasm_bindgen::JsCast;

    pub fn canvas_size() -> (u32, u32) {
        web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.body())
            .map(|body| (body.client_width() as u32, body.client_height() as u32))
            .unwrap_or((800, 600))
    }

    pub fn set_canvas_size(width: u32, height: u32) {
        if let Some(canvas) = get_canvas_element() {
            canvas.set_width(width);
            canvas.set_height(height);
        }
    }

    pub fn get_canvas_element() -> Option<web_sys::HtmlCanvasElement> {
        use web_sys::window;

        window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("canvas"))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
    }

    pub fn request_animation_frame(f: &js_sys::Function) {
        if let Some(window) = web_sys::window() {
            let _ = window.request_animation_frame(f);
        }
    }

    pub struct WasmRuntime {
        frame_count: u64,
        last_frame_time: f64,
        fps: f32,
        fps_accumulator: f32,
        fps_frame_count: u32,
    }

    impl WasmRuntime {
        pub fn new() -> Self {
            Self {
                frame_count: 0,
                last_frame_time: now_secs(),
                fps: 0.0,
                fps_accumulator: 0.0,
                fps_frame_count: 0,
            }
        }

        pub fn begin_frame(&mut self) {
            let now = now_secs();
            let delta = now - self.last_frame_time;
            self.last_frame_time = now;

            self.fps_accumulator += delta as f32;
            self.fps_frame_count += 1;

            if self.fps_accumulator >= 1.0 {
                self.fps = self.fps_frame_count as f32 / self.fps_accumulator;
                self.fps_accumulator = 0.0;
                self.fps_frame_count = 0;
            }

            self.frame_count += 1;
        }

        pub fn frame_count(&self) -> u64 {
            self.frame_count
        }

        pub fn fps(&self) -> f32 {
            self.fps
        }

        pub fn delta_time(&self) -> f32 {
            1.0 / 60.0
        }
    }

    impl Default for WasmRuntime {
        fn default() -> Self {
            Self::new()
        }
    }

    pub fn fetch_text(url: &str) -> impl std::future::Future<Output = Result<String, String>> {
        let url = url.to_string();

        async move {
            use wasm_bindgen_futures::JsFuture;

            let window = web_sys::window().ok_or("No window")?;
            let response = JsFuture::from(window.fetch_with_str(&url))
                .await
                .map_err(|e| format!("Fetch failed: {:?}", e))?;

            let response: web_sys::Response = response
                .dyn_into()
                .map_err(|e| format!("Not a response: {:?}", e))?;

            let text = JsFuture::from(
                response
                    .text()
                    .map_err(|e| format!("text() failed: {:?}", e))?,
            )
            .await
            .map_err(|e| format!("Text read failed: {:?}", e))?;

            Ok(text.as_string().unwrap_or_default())
        }
    }

    pub fn fetch_bytes(url: &str) -> impl std::future::Future<Output = Result<Vec<u8>, String>> {
        let url = url.to_string();

        async move {
            use wasm_bindgen_futures::JsFuture;

            let window = web_sys::window().ok_or("No window")?;
            let response = JsFuture::from(window.fetch_with_str(&url))
                .await
                .map_err(|e| format!("Fetch failed: {:?}", e))?;

            let response: web_sys::Response = response
                .dyn_into()
                .map_err(|e| format!("Not a response: {:?}", e))?;

            let array_buffer = JsFuture::from(
                response
                    .array_buffer()
                    .map_err(|e| format!("array_buffer() failed: {:?}", e))?,
            )
            .await
            .map_err(|e| format!("ArrayBuffer read failed: {:?}", e))?;

            let array = js_sys::Uint8Array::new(&array_buffer);
            Ok(array.to_vec())
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub mod native {
    pub fn now_ms() -> f64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    pub fn now_secs() -> f64 {
        now_ms() / 1000.0
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm32::*;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
