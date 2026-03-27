//! Build utilities tests

#[test]
fn build_config_default() {
    use quasar_build::BuildConfig;

    let config = BuildConfig::default();
    assert!(!config.release);
}

#[test]
fn build_config_release() {
    use quasar_build::BuildConfig;

    let config = BuildConfig {
        release: true,
        ..Default::default()
    };
    assert!(config.release);
}

#[test]
fn asset_config_default() {
    use quasar_build::AssetConfig;

    let config = AssetConfig::default();
    assert!(!config.compress_textures);
}
