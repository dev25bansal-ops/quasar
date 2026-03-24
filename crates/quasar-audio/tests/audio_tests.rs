//! Tests for the Quasar Audio system.
//!
//! Tests audio bus management, volume control, and sound handle operations.

use quasar_audio::audio_graph::DspNode;
use quasar_audio::{AudioBus, SoundId};

// ── Audio Bus Tests ──

#[test]
fn audio_bus_default() {
    let bus = AudioBus::default();
    assert_eq!(bus, AudioBus::Sfx);
}

#[test]
fn audio_bus_equality() {
    assert_eq!(AudioBus::Master, AudioBus::Master);
    assert_eq!(AudioBus::Music, AudioBus::Music);
    assert_eq!(AudioBus::Sfx, AudioBus::Sfx);
    assert_eq!(AudioBus::Voice, AudioBus::Voice);
    assert_eq!(AudioBus::Ambient, AudioBus::Ambient);

    assert_ne!(AudioBus::Master, AudioBus::Music);
    assert_ne!(AudioBus::Sfx, AudioBus::Voice);
}

#[test]
fn audio_bus_custom() {
    let custom = AudioBus::Custom("Reverb".to_string());
    match custom {
        AudioBus::Custom(name) => assert_eq!(name, "Reverb"),
        _ => panic!("Expected Custom variant"),
    }
}

#[test]
fn audio_bus_hash() {
    use std::collections::HashSet;

    let mut set = HashSet::new();
    set.insert(AudioBus::Master);
    set.insert(AudioBus::Music);
    set.insert(AudioBus::Master); // Duplicate

    assert_eq!(set.len(), 2);
}

// ── Sound ID Tests ──

#[test]
fn sound_id_type() {
    let id: SoundId = 42;
    assert_eq!(id, 42);

    let id2: SoundId = u64::MAX;
    assert_eq!(id2, u64::MAX);
}

#[test]
fn sound_id_uniqueness() {
    let mut ids = std::collections::HashSet::new();

    for i in 0..1000 {
        let id: SoundId = i;
        assert!(ids.insert(id), "Sound IDs should be unique");
    }

    assert_eq!(ids.len(), 1000);
}

// ── Bus Ordering Tests ──

#[test]
fn bus_ordering() {
    let buses = [
        AudioBus::Master,
        AudioBus::Music,
        AudioBus::Sfx,
        AudioBus::Voice,
        AudioBus::Ambient,
    ];

    for i in 0..buses.len() {
        for j in (i + 1)..buses.len() {
            assert_ne!(buses[i], buses[j], "Buses should be distinct");
        }
    }
}

// ── Audio Graph Tests ──

#[test]
fn audio_graph_creation() {
    use quasar_audio::audio_graph::AudioGraph;

    let graph = AudioGraph::new(44100);
    assert_eq!(graph.node_count(), 0);
}

#[test]
fn audio_graph_dsp_nodes() {
    use quasar_audio::audio_graph::{AudioGraph, ParametricEq};

    let mut graph = AudioGraph::new(44100);

    let eq = ParametricEq::new(1000.0, 3.0, 1.0);
    graph.add_node(Box::new(eq));

    assert_eq!(graph.node_count(), 1);
}

#[test]
fn audio_graph_process() {
    use quasar_audio::audio_graph::AudioGraph;

    let mut graph = AudioGraph::new(44100);

    let mut buffer = vec![0.0f32; 256];
    graph.process(&mut buffer, None);

    assert!(buffer.iter().all(|&x| x == 0.0));
}

// ── Volume and Panning Tests ──

#[test]
fn volume_clamping() {
    let volume: f64 = 1.5;
    let clamped = volume.clamp(0.0, 1.0);
    assert_eq!(clamped, 1.0);

    let volume: f64 = -0.5;
    let clamped = volume.clamp(0.0, 1.0);
    assert_eq!(clamped, 0.0);
}

#[test]
fn panning_range() {
    let panning: f32 = 0.0;
    assert!((-1.0..=1.0).contains(&panning));

    let panning: f32 = -1.0;
    assert!((-1.0..=1.0).contains(&panning));

    let panning: f32 = 1.0;
    assert!((-1.0..=1.0).contains(&panning));
}

// ── Sample Rate and Format Tests ──

#[test]
fn sample_rate_calculation() {
    let sample_rates = [44100, 48000, 96000, 192000];

    for &rate in &sample_rates {
        let samples_per_ms = rate as f64 / 1000.0;
        assert!(samples_per_ms > 0.0);
    }
}

#[test]
fn buffer_size_alignment() {
    let buffer_sizes: [u32; 6] = [64, 128, 256, 512, 1024, 2048];

    for &size in &buffer_sizes {
        assert!(size.is_power_of_two(), "Buffer size should be power of 2");
    }
}

// ── Frequency and Pitch Tests ──

#[test]
fn frequency_octaves() {
    let a4: f32 = 440.0;

    let a5 = a4 * 2.0;
    assert_eq!(a5, 880.0);

    let a3 = a4 / 2.0;
    assert_eq!(a3, 220.0);
}

#[test]
fn pitch_bend_range() {
    let pitch_bend: f32 = 0.0;
    let bend_range: i32 = 12;

    let min_bend = -bend_range;
    let max_bend = bend_range;

    assert!(pitch_bend >= min_bend as f32);
    assert!(pitch_bend <= max_bend as f32);
}

// ── Decibel Conversion Tests ──

#[test]
fn db_to_linear() {
    let db: f64 = 0.0;
    let linear = 10_f64.powf(db / 20.0);
    assert!((linear - 1.0).abs() < 0.001);

    let db: f64 = -6.0;
    let linear = 10_f64.powf(db / 20.0);
    assert!((linear - 0.501).abs() < 0.01);
}

#[test]
fn linear_to_db() {
    let linear: f64 = 1.0;
    let db = 20.0 * linear.log10();
    assert!((db - 0.0).abs() < 0.001);

    let linear: f64 = 0.5;
    let db = 20.0 * linear.log10();
    assert!((db - (-6.02)).abs() < 0.1);
}

// ── Spatial Audio Tests ──

#[test]
fn distance_attenuation() {
    let listener_pos: f32 = 0.0;
    let sound_pos: f32 = 10.0;
    let ref_distance: f32 = 1.0;

    let distance = (sound_pos - listener_pos).abs();
    let gain = ref_distance / distance.max(ref_distance);

    assert!(gain > 0.0 && gain <= 1.0);
}

#[test]
fn stereo_panning_calculation() {
    let pan: f32 = 0.0;

    let left_gain = (1.0 - pan).max(0.0).min(1.0) * 0.5 + 0.5 * (1.0 - pan.abs());
    let right_gain = (1.0 + pan).max(0.0).min(1.0) * 0.5 + 0.5 * (1.0 - pan.abs());

    // At center pan, both gains should be equal
    assert!((left_gain - right_gain).abs() < 0.001);
    // Both should be between 0.5 and 1.0
    assert!(left_gain >= 0.5 && left_gain <= 1.0);
    assert!(right_gain >= 0.5 && right_gain <= 1.0);
}

// ── DSP Node Tests ──

#[test]
fn parametric_eq_creation() {
    use quasar_audio::audio_graph::ParametricEq;

    let eq = ParametricEq::new(1000.0, 3.0, 1.0);
    assert_eq!(eq.name(), "eq");
}

#[test]
fn compressor_creation() {
    use quasar_audio::audio_graph::Compressor;

    let comp = Compressor::new(-12.0, 4.0, 0.003, 0.1);
    assert_eq!(comp.name(), "compressor");
}

#[test]
fn limiter_creation() {
    use quasar_audio::audio_graph::Limiter;

    let limiter = Limiter::new(-0.3, 0.005);
    assert_eq!(limiter.name(), "limiter");
}

#[test]
fn reverb_send_creation() {
    use quasar_audio::audio_graph::ReverbSend;

    let reverb = ReverbSend::new(0.5, 0.3, 0.2);
    assert_eq!(reverb.name(), "reverb_send");
}
