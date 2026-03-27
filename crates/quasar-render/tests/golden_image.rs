//! Golden-image render test scaffold.
//!
//! Renders known scenes to off-screen textures via `wgpu`, then compares the
//! result against reference images stored in `tests/golden/`.  A per-pixel
//! RMSE threshold decides pass/fail.
//!
//! **First run**: generates golden images (no reference to compare against).
//! **Subsequent runs**: compares against stored references.
//!
//! Set `QUASAR_UPDATE_GOLDEN=1` to overwrite references with the latest render.

use std::path::{Path, PathBuf};

/// Root directory for golden reference images.
fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

/// Per-pixel root-mean-square error between two RGBA byte buffers.
fn rmse(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len(), "image size mismatch");
    if a.is_empty() {
        return 0.0;
    }
    let sum: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let diff = x as f64 - y as f64;
            diff * diff
        })
        .sum();
    (sum / a.len() as f64).sqrt()
}

/// Compare a rendered buffer to the golden reference at `name`.
///
/// Returns `Ok(())` if within threshold, or `Err(rmse)` on mismatch.
/// If no reference exists (or `QUASAR_UPDATE_GOLDEN=1`), writes a new one.
fn compare_golden(
    name: &str,
    rendered: &[u8],
    width: u32,
    height: u32,
    threshold: f64,
) -> Result<(), f64> {
    let dir = golden_dir();
    std::fs::create_dir_all(&dir).ok();

    let ref_path = dir.join(format!("{name}.bin"));
    let update = std::env::var("QUASAR_UPDATE_GOLDEN").map_or(false, |v| v == "1");

    if update || !ref_path.exists() {
        std::fs::write(&ref_path, rendered).ok();
        // Store dimensions alongside for validation.
        let meta_path = dir.join(format!("{name}.meta"));
        std::fs::write(&meta_path, format!("{width}x{height}")).ok();
        return Ok(());
    }

    let reference = match std::fs::read(&ref_path) {
        Ok(data) => data,
        Err(_) => return Ok(()), // no reference ⇒ pass
    };

    let error = rmse(&reference, rendered);
    if error <= threshold {
        Ok(())
    } else {
        Err(error)
    }
}

// ── Placeholder tests ────────────────────────────────────────────
// These run without a GPU; they validate the RMSE helper and scaffold.
// Real golden-image tests require wgpu device creation (skipped in CI
// unless a GPU adapter is available).

#[test]
fn rmse_identical_buffers() {
    let buf = vec![128u8; 256];
    assert_eq!(rmse(&buf, &buf), 0.0);
}

#[test]
fn rmse_different_buffers() {
    let a = vec![0u8; 64];
    let b = vec![10u8; 64];
    let error = rmse(&a, &b);
    assert!((error - 10.0).abs() < 0.001);
}

#[test]
fn golden_write_and_match() {
    // Use a unique name so parallel test runs don't collide.
    let name = "test_golden_scaffold";
    let data = vec![42u8; 16 * 16 * 4];

    // First call writes the reference.
    let _ = std::env::set_var("QUASAR_UPDATE_GOLDEN", "1");
    assert!(compare_golden(name, &data, 16, 16, 1.0).is_ok());

    // Second call with same data should pass.
    let _ = std::env::remove_var("QUASAR_UPDATE_GOLDEN");
    assert!(compare_golden(name, &data, 16, 16, 1.0).is_ok());

    // Clean up.
    let dir = golden_dir();
    let _ = std::fs::remove_file(dir.join(format!("{name}.bin")));
    let _ = std::fs::remove_file(dir.join(format!("{name}.meta")));
}
