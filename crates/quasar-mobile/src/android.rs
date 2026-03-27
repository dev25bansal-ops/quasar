//! Android platform support.
//!
//! Provides JNI bridge to Android AssetManager for loading assets
//! packaged in the APK's assets/ directory.

use std::path::Path;
use std::sync::OnceLock;

use jni::objects::{JByteArray, JClass, JObject, JString, JValue};
use jni::JNIEnv;

static ASSET_MANAGER: OnceLock<AssetManager> = OnceLock::new();

/// Android AssetManager wrapper for reading assets from APK.
pub struct AssetManager {
    env: JNIEnv<'static>,
    asset_manager: JObject<'static>,
}

impl quasar_core::platform::AssetLoader for AssetManager {
    async fn read_asset(&self, path: &Path) -> Result<Vec<u8>, String> {
        let path_str = path.to_string_lossy().into_owned();
        self.read_asset(&path_str)
    }

    fn asset_exists(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy().into_owned();
        self.asset_exists(&path_str)
    }

    fn list_assets(&self, path: &Path) -> Result<Vec<String>, String> {
        let path_str = path.to_string_lossy().into_owned();
        self.list_assets(&path_str)
    }
}

impl AssetManager {
    /// Initialize the AssetManager from the Android context.
    ///
    /// This should be called once during app startup, typically from
    /// `android_main()` or the activity's `onCreate()`.
    ///
    /// # Safety
    ///
    /// The JNIEnv must be valid and attached to the current thread.
    /// The AssetManager Java object must be valid for the lifetime of the app.
    pub unsafe fn init(
        env: JNIEnv<'static>,
        asset_manager: JObject<'static>,
    ) -> Result<(), String> {
        let manager = Self { env, asset_manager };
        ASSET_MANAGER
            .set(manager)
            .map_err(|_| "AssetManager already initialized".to_string())
    }

    /// Get the global AssetManager instance.
    pub fn get() -> Option<&'static Self> {
        ASSET_MANAGER.get()
    }

    /// Read an asset file as bytes.
    ///
    /// # Arguments
    ///
    /// * `path` - Path relative to the assets/ directory in the APK.
    ///
    /// # Returns
    ///
    /// The file contents as a Vec<u8>, or an error message.
    pub fn read_asset(&self, path: &str) -> Result<Vec<u8>, String> {
        let path_jstr = JString::from(self.env.new_string(path).map_err(|e| e.to_string())?);

        // Call AssetManager.open(String) -> InputStream
        let input_stream = self
            .env
            .call_method(
                &self.asset_manager,
                "open",
                "(Ljava/lang/String;)Ljava/io/InputStream;",
                &[JValue::Object(&path_jstr.into())],
            )
            .map_err(|e| format!("Failed to open asset '{}': {}", path, e))?
            .l()
            .map_err(|e| e.to_string())?;

        if input_stream.is_null() {
            return Err(format!("Asset not found: {}", path));
        }

        // Read all bytes from the InputStream
        let bytes = self.read_all_bytes(&input_stream)?;

        // Close the input stream
        let _: () = self
            .env
            .call_method(&input_stream, "close", "()V", &[])
            .map_err(|e| format!("Failed to close stream: {}", e))?
            .try_into()
            .map_err(|e| e.to_string())?;

        Ok(bytes)
    }

    /// Read all bytes from a Java InputStream.
    fn read_all_bytes(&self, input_stream: &JObject) -> Result<Vec<u8>, String> {
        // Create a ByteArrayOutputStream
        let baos_class: JClass = self
            .env
            .find_class("java/io/ByteArrayOutputStream")
            .map_err(|e| e.to_string())?;

        let baos = self
            .env
            .new_object(baos_class, "()V", &[])
            .map_err(|e| e.to_string())?;

        // Create a 4KB buffer
        let buffer = self.env.new_byte_array(4096).map_err(|e| e.to_string())?;

        let buffer_obj = JObject::from(buffer);

        // Read loop
        loop {
            let read = self
                .env
                .call_method(
                    input_stream,
                    "read",
                    "([B)I",
                    &[JValue::Object(&buffer_obj)],
                )
                .map_err(|e| e.to_string())?
                .i()
                .map_err(|e| e.to_string())?;

            if read == -1 {
                break;
            }

            // Write to ByteArrayOutputStream
            let _: () = self
                .env
                .call_method(
                    &baos,
                    "write",
                    "([BI)V",
                    &[JValue::Object(&buffer_obj), JValue::Int(read)],
                )
                .map_err(|e| e.to_string())?
                .try_into()
                .map_err(|e| e.to_string())?;
        }

        // Get the byte array from ByteArrayOutputStream
        let result: JByteArray = self
            .env
            .call_method(&baos, "toByteArray", "()[B", &[])
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|e| e.to_string())?;

        // Convert to Rust Vec<u8>
        let len = self
            .env
            .get_array_length(&result)
            .map_err(|e| e.to_string())?;
        let mut bytes = vec![0u8; len as usize];
        self.env
            .get_byte_array_region(result, 0, &mut unsafe {
                std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut i8, bytes.len())
            })
            .map_err(|e| e.to_string())?;

        // Convert i8 to u8
        Ok(bytes)
    }

    /// List assets in a directory.
    ///
    /// # Arguments
    ///
    /// * `path` - Directory path relative to assets/ (empty string for root).
    ///
    /// # Returns
    ///
    /// A vector of asset/file names in the directory.
    pub fn list_assets(&self, path: &str) -> Result<Vec<String>, String> {
        let path_jstr = JString::from(self.env.new_string(path).map_err(|e| e.to_string())?);

        // Call AssetManager.list(String) -> String[]
        let result = self
            .env
            .call_method(
                &self.asset_manager,
                "list",
                "(Ljava/lang/String;)[Ljava/lang/String;",
                &[JValue::Object(&path_jstr.into())],
            )
            .map_err(|e| format!("Failed to list assets: {}", e))?;

        let array: jni::objects::JObjectArray = result.l().map_err(|e| e.to_string())?.into();

        let len = self
            .env
            .get_array_length(&array)
            .map_err(|e| e.to_string())?;
        let mut names = Vec::with_capacity(len as usize);

        for i in 0..len {
            let elem: JString = self
                .env
                .get_object_array_element(&array, i)
                .map_err(|e| e.to_string())?
                .into();
            let name: String = self
                .env
                .get_string(&elem)
                .map_err(|e| e.to_string())?
                .into();
            names.push(name);
        }

        Ok(names)
    }

    /// Check if an asset exists.
    pub fn asset_exists(&self, path: &str) -> bool {
        if path.is_empty() {
            return true;
        }

        // Extract parent directory and filename
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));

        match self.list_assets(parent) {
            Ok(names) => names.iter().any(|n| n == name),
            Err(_) => false,
        }
    }
}

/// Read an asset file asynchronously.
///
/// This is a convenience function that wraps the synchronous AssetManager
/// in an async API for consistency with other platform file I/O.
pub async fn read_asset(path: &Path) -> Result<Vec<u8>, String> {
    let path_str = path.to_string_lossy().into_owned();

    AssetManager::get()
        .ok_or_else(|| "AssetManager not initialized".to_string())?
        .read_asset(&path_str)
}

/// Check if an asset exists.
pub fn asset_exists(path: &Path) -> bool {
    let path_str = path.to_string_lossy().into_owned();

    AssetManager::get()
        .map(|am| am.asset_exists(&path_str))
        .unwrap_or(false)
}

/// List all assets in a directory.
pub fn list_assets(path: &Path) -> Result<Vec<String>, String> {
    let path_str = path.to_string_lossy().into_owned();

    AssetManager::get()
        .ok_or_else(|| "AssetManager not initialized".to_string())?
        .list_assets(&path_str)
}

/// Initialize the AssetManager from the current Android context.
///
/// This function retrieves the Application context and its AssetManager.
/// It should be called once during app startup.
///
/// # Safety
///
/// This function uses unsafe code to obtain a 'static reference to the
/// JNIEnv and AssetManager. The caller must ensure the Android activity
/// remains valid for the lifetime of the application.
#[allow(clippy::unwrap_used)]
pub unsafe fn init_asset_manager() -> Result<(), String> {
    let context = ndk_context::android_context();
    let activity = context.context().cast::<JObject>();

    let vm = context.vm();
    let mut env = vm
        .get_env()
        .map_err(|e| format!("Failed to get JNIEnv: {:?}", e))?;

    // Get Application context (the activity is an instance of Context)
    let app_context = env
        .call_method(
            activity,
            "getApplicationContext",
            "()Landroid/content/Context;",
            &[],
        )
        .map_err(|e| format!("Failed to get application context: {}", e))?
        .l()
        .map_err(|e| e.to_string())?;

    // Get AssetManager from context
    let asset_manager = env
        .call_method(
            &app_context,
            "getAssets",
            "()Landroid/content/res/AssetManager;",
            &[],
        )
        .map_err(|e| format!("Failed to get AssetManager: {}", e))?
        .l()
        .map_err(|e| e.to_string())?;

    // Convert to 'static references
    // SAFETY: We're extending the lifetime because the AssetManager is valid
    // for the entire application lifetime
    let env_static: JNIEnv<'static> = std::mem::transmute(env);
    let asset_manager_static: JObject<'static> = std::mem::transmute(asset_manager);

    // Initialize the AssetManager
    let manager = AssetManager {
        env: env_static,
        asset_manager: asset_manager_static,
    };

    ASSET_MANAGER
        .set(manager)
        .map_err(|_| "AssetManager already initialized".to_string())?;

    // Register with quasar-core
    let manager_ref = ASSET_MANAGER.get().unwrap();
    quasar_core::platform::register_asset_loader(Box::new(AssetLoaderAdapter(
        manager_ref as *const AssetManager,
    )))
    .map_err(|_| "Failed to register asset loader")?;

    Ok(())
}

/// Adapter to wrap the static AssetManager reference for the trait impl.
struct AssetLoaderAdapter(*const AssetManager);

impl quasar_core::platform::AssetLoader for AssetLoaderAdapter {
    async fn read_asset(&self, path: &Path) -> Result<Vec<u8>, String> {
        unsafe { (*self.0).read_asset(&path.to_string_lossy()) }
    }

    fn asset_exists(&self, path: &Path) -> bool {
        unsafe { (*self.0).asset_exists(&path.to_string_lossy()) }
    }

    fn list_assets(&self, path: &Path) -> Result<Vec<String>, String> {
        unsafe { (*self.0).list_assets(&path.to_string_lossy()) }
    }
}

unsafe impl Send for AssetLoaderAdapter {}
unsafe impl Sync for AssetLoaderAdapter {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_asset_manager_not_initialized() {
        assert!(AssetManager::get().is_none());
    }
}
