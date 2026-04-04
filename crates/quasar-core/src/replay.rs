//! Replay System - Record and Playback Game Sessions
//!
//! This module provides functionality to record and playback game sessions,
//! useful for:
//! - Bug reproduction and debugging
//! - Highlight/glider recording
//! - Spectator mode with time controls
//! - QA and automated testing
//!
//! ## Basic Usage
//!
//! ```rust,ignore
//! use quasar_core::replay::{ReplayRecorder, ReplayPlayer};
//!
//! // Recording
//! let mut recorder = ReplayRecorder::new("level_1");
//! recorder.record_frame(delta_time, |snapshot| {
//!     snapshot.set_entity_positions(&world);
//!     snapshot.set_player_inputs(&input_buffer);
//! });
//!
//! // Playback
//! let mut player = ReplayPlayer::new(recorder.finish());
//! player.play();
//! player.set_speed(2.0); // 2x speed
//! player.update(delta_time, |frame| {
//!     frame.apply_to_world(&mut world);
//! });
//! ```

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::time::Duration;

pub const REPLAY_VERSION: u32 = 1;
pub const MAX_REPLAY_FRAMES: usize = 3600 * 60; // 1 hour at 60 FPS

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayState {
    Recording,
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayHeader {
    pub version: u32,
    pub game_version: String,
    pub map_name: String,
    pub recorded_at: u64,
    pub duration_frames: u32,
    pub fps: f32,
    pub player_name: Option<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

impl Default for ReplayHeader {
    fn default() -> Self {
        Self {
            version: REPLAY_VERSION,
            game_version: env!("CARGO_PKG_VERSION").to_string(),
            map_name: String::new(),
            recorded_at: 0,
            duration_frames: 0,
            fps: 60.0,
            player_name: None,
            metadata: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntitySnapshot {
    pub entity_id: u64,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub velocity: Option<[f32; 3]>,
    pub animation_state: Option<u32>,
    pub custom_data: Vec<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputFrame {
    pub frame_number: u32,
    pub inputs: Vec<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplayFrame {
    pub frame_number: u32,
    pub timestamp: f32,
    pub entities: Vec<EntitySnapshot>,
    pub inputs: Option<InputFrame>,
    pub events: Vec<ReplayEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayEvent {
    pub event_type: String,
    pub timestamp: f32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayData {
    pub header: ReplayHeader,
    pub frames: Vec<ReplayFrame>,
    pub key_frames: Vec<u32>,
}

impl ReplayData {
    pub fn new(header: ReplayHeader) -> Self {
        Self {
            header,
            frames: Vec::new(),
            key_frames: Vec::new(),
        }
    }

    pub fn duration_secs(&self) -> f32 {
        self.header.duration_frames as f32 / self.header.fps
    }

    pub fn frame_count(&self) -> u32 {
        self.frames.len() as u32
    }

    pub fn get_frame(&self, frame_number: u32) -> Option<&ReplayFrame> {
        self.frames.iter().find(|f| f.frame_number == frame_number)
    }

    pub fn get_frame_at_time(&self, time_secs: f32) -> Option<&ReplayFrame> {
        let target_frame = (time_secs * self.header.fps) as u32;
        self.get_frame(target_frame)
    }

    pub fn find_nearest_keyframe(&self, frame_number: u32) -> Option<u32> {
        self.key_frames
            .iter()
            .filter(|&&f| f <= frame_number)
            .max()
            .copied()
    }

    pub fn serialize(&self) -> Result<Vec<u8>, ReplayError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
            .map_err(|e| ReplayError::SerializationError(e.to_string()))
    }

    pub fn deserialize(data: &[u8]) -> Result<Self, ReplayError> {
        bincode::serde::decode_from_slice(data, bincode::config::standard())
            .map(|(v, _)| v)
            .map_err(|e| ReplayError::DeserializationError(e.to_string()))
    }

    pub fn save_to_file(&self, path: &std::path::Path) -> Result<(), ReplayError> {
        let data = self.serialize()?;
        let mut file =
            std::fs::File::create(path).map_err(|e| ReplayError::IoError(e.to_string()))?;
        file.write_all(&data)
            .map_err(|e| ReplayError::IoError(e.to_string()))?;
        Ok(())
    }

    pub fn load_from_file(path: &std::path::Path) -> Result<Self, ReplayError> {
        let mut file =
            std::fs::File::open(path).map_err(|e| ReplayError::IoError(e.to_string()))?;
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .map_err(|e| ReplayError::IoError(e.to_string()))?;
        Self::deserialize(&data)
    }
}

pub struct ReplayRecorder {
    data: ReplayData,
    current_frame: u32,
    keyframe_interval: u32,
    state: ReplayState,
    entity_filter: Option<Box<dyn Fn(u64) -> bool + Send + Sync>>,
}

impl ReplayRecorder {
    pub fn new(map_name: &str) -> Self {
        let header = ReplayHeader {
            map_name: map_name.to_string(),
            recorded_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_secs(),
            ..Default::default()
        };

        Self {
            data: ReplayData::new(header),
            current_frame: 0,
            keyframe_interval: 60,
            state: ReplayState::Recording,
            entity_filter: None,
        }
    }

    pub fn with_fps(mut self, fps: f32) -> Self {
        self.data.header.fps = fps;
        self
    }

    pub fn with_player_name(mut self, name: &str) -> Self {
        self.data.header.player_name = Some(name.to_string());
        self
    }

    pub fn with_keyframe_interval(mut self, interval: u32) -> Self {
        self.keyframe_interval = interval;
        self
    }

    pub fn with_metadata(mut self, key: &str, value: &str) -> Self {
        self.data
            .header
            .metadata
            .insert(key.to_string(), value.to_string());
        self
    }

    pub fn set_entity_filter<F: Fn(u64) -> bool + Send + Sync + 'static>(&mut self, filter: F) {
        self.entity_filter = Some(Box::new(filter));
    }

    pub fn record_frame(&mut self, _delta_time: f32) -> FrameRecorder<'_> {
        self.current_frame += 1;
        self.data.header.duration_frames = self.current_frame;

        if self.current_frame % self.keyframe_interval == 0 {
            self.data.key_frames.push(self.current_frame);
        }

        FrameRecorder {
            frame: ReplayFrame {
                frame_number: self.current_frame,
                timestamp: self.current_frame as f32 / self.data.header.fps,
                entities: Vec::new(),
                inputs: None,
                events: Vec::new(),
            },
            recorder: self,
        }
    }

    pub fn record_event(&mut self, event_type: &str, data: Vec<u8>) {
        if let Some(last_frame) = self.data.frames.last_mut() {
            last_frame.events.push(ReplayEvent {
                event_type: event_type.to_string(),
                timestamp: last_frame.timestamp,
                data,
            });
        }
    }

    pub fn frame_count(&self) -> u32 {
        self.current_frame
    }

    pub fn duration_secs(&self) -> f32 {
        self.current_frame as f32 / self.data.header.fps
    }

    pub fn finish(mut self) -> ReplayData {
        self.state = ReplayState::Stopped;
        self.data
    }

    pub fn is_recording(&self) -> bool {
        self.state == ReplayState::Recording
    }
}

pub struct FrameRecorder<'a> {
    frame: ReplayFrame,
    recorder: &'a mut ReplayRecorder,
}

impl<'a> FrameRecorder<'a> {
    pub fn add_entity(
        &mut self,
        entity_id: u64,
        position: [f32; 3],
        rotation: [f32; 4],
    ) -> &mut Self {
        if let Some(ref filter) = self.recorder.entity_filter {
            if !filter(entity_id) {
                return self;
            }
        }

        self.frame.entities.push(EntitySnapshot {
            entity_id,
            position,
            rotation,
            velocity: None,
            animation_state: None,
            custom_data: Vec::new(),
        });
        self
    }

    pub fn add_entity_with_velocity(
        &mut self,
        entity_id: u64,
        position: [f32; 3],
        rotation: [f32; 4],
        velocity: [f32; 3],
    ) -> &mut Self {
        if let Some(ref filter) = self.recorder.entity_filter {
            if !filter(entity_id) {
                return self;
            }
        }

        self.frame.entities.push(EntitySnapshot {
            entity_id,
            position,
            rotation,
            velocity: Some(velocity),
            animation_state: None,
            custom_data: Vec::new(),
        });
        self
    }

    pub fn set_inputs(&mut self, inputs: Vec<u8>) -> &mut Self {
        self.frame.inputs = Some(InputFrame {
            frame_number: self.frame.frame_number,
            inputs,
        });
        self
    }

    pub fn add_event(&mut self, event_type: &str, data: Vec<u8>) -> &mut Self {
        self.frame.events.push(ReplayEvent {
            event_type: event_type.to_string(),
            timestamp: self.frame.timestamp,
            data,
        });
        self
    }
}

impl Drop for FrameRecorder<'_> {
    fn drop(&mut self) {
        if self.recorder.data.frames.len() < MAX_REPLAY_FRAMES {
            self.recorder.data.frames.push(self.frame.clone());
        }
    }
}

pub struct ReplayPlayer {
    data: ReplayData,
    current_frame: u32,
    playback_time: f32,
    speed: f32,
    state: ReplayState,
    loop_enabled: bool,
}

impl ReplayPlayer {
    pub fn new(data: ReplayData) -> Self {
        Self {
            data,
            current_frame: 0,
            playback_time: 0.0,
            speed: 1.0,
            state: ReplayState::Paused,
            loop_enabled: false,
        }
    }

    pub fn play(&mut self) {
        self.state = ReplayState::Playing;
    }

    pub fn pause(&mut self) {
        self.state = ReplayState::Paused;
    }

    pub fn stop(&mut self) {
        self.state = ReplayState::Stopped;
        self.current_frame = 0;
        self.playback_time = 0.0;
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.1, 10.0);
    }

    pub fn set_loop(&mut self, enabled: bool) {
        self.loop_enabled = enabled;
    }

    pub fn seek_to_frame(&mut self, frame: u32) {
        self.current_frame = frame.min(self.data.frame_count().saturating_sub(1));
        self.playback_time = self.current_frame as f32 / self.data.header.fps;
    }

    pub fn seek_to_time(&mut self, time_secs: f32) {
        self.playback_time = time_secs.clamp(0.0, self.data.duration_secs());
        self.current_frame = (self.playback_time * self.data.header.fps) as u32;
    }

    pub fn seek_to_percent(&mut self, percent: f32) {
        let target_time = (percent / 100.0) * self.data.duration_secs();
        self.seek_to_time(target_time);
    }

    pub fn update(&mut self, delta_time: f32) -> Option<&ReplayFrame> {
        if self.state != ReplayState::Playing {
            return self.data.get_frame(self.current_frame);
        }

        self.playback_time += delta_time * self.speed;

        if self.playback_time >= self.data.duration_secs() {
            if self.loop_enabled {
                self.playback_time = 0.0;
                self.current_frame = 0;
            } else {
                self.state = ReplayState::Stopped;
                return None;
            }
        }

        self.current_frame = (self.playback_time * self.data.header.fps) as u32;
        self.data.get_frame(self.current_frame)
    }

    pub fn current_frame(&self) -> u32 {
        self.current_frame
    }

    pub fn current_time(&self) -> f32 {
        self.playback_time
    }

    pub fn progress_percent(&self) -> f32 {
        if self.data.duration_secs() > 0.0 {
            (self.playback_time / self.data.duration_secs()) * 100.0
        } else {
            0.0
        }
    }

    pub fn remaining_time(&self) -> f32 {
        (self.data.duration_secs() - self.playback_time).max(0.0)
    }

    pub fn is_playing(&self) -> bool {
        self.state == ReplayState::Playing
    }

    pub fn is_finished(&self) -> bool {
        self.state == ReplayState::Stopped && self.playback_time >= self.data.duration_secs()
    }

    pub fn header(&self) -> &ReplayHeader {
        &self.data.header
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }
}

#[derive(Debug, Clone)]
pub enum ReplayError {
    SerializationError(String),
    DeserializationError(String),
    IoError(String),
    InvalidVersion(u32),
    CorruptedData(String),
    FrameNotFound(u32),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SerializationError(e) => write!(f, "Replay serialization error: {}", e),
            Self::DeserializationError(e) => write!(f, "Replay deserialization error: {}", e),
            Self::IoError(e) => write!(f, "Replay I/O error: {}", e),
            Self::InvalidVersion(v) => write!(f, "Invalid replay version: {}", v),
            Self::CorruptedData(e) => write!(f, "Corrupted replay data: {}", e),
            Self::FrameNotFound(frame) => write!(f, "Frame {} not found in replay", frame),
        }
    }
}

impl std::error::Error for ReplayError {}

pub struct ReplayManager {
    recorder: Option<ReplayRecorder>,
    player: Option<ReplayPlayer>,
}

impl ReplayManager {
    pub fn new() -> Self {
        Self {
            recorder: None,
            player: None,
        }
    }

    pub fn start_recording(&mut self, map_name: &str) {
        self.recorder = Some(ReplayRecorder::new(map_name));
        self.player = None;
    }

    pub fn stop_recording(&mut self) -> Option<ReplayData> {
        self.recorder.take().map(|r| r.finish())
    }

    pub fn start_playback(&mut self, data: ReplayData) {
        self.player = Some(ReplayPlayer::new(data));
        self.recorder = None;
    }

    pub fn stop_playback(&mut self) {
        self.player = None;
    }

    pub fn recorder(&mut self) -> Option<&mut ReplayRecorder> {
        self.recorder.as_mut()
    }

    pub fn player(&mut self) -> Option<&mut ReplayPlayer> {
        self.player.as_mut()
    }

    pub fn is_recording(&self) -> bool {
        self.recorder.as_ref().is_some_and(|r| r.is_recording())
    }

    pub fn is_playing(&self) -> bool {
        self.player.as_ref().is_some_and(|p| p.is_playing())
    }
}

impl Default for ReplayManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_recorder_basic() {
        let mut recorder = ReplayRecorder::new("test_map");

        {
            let mut frame = recorder.record_frame(1.0 / 60.0);
            frame.add_entity(1, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]);
            frame.set_inputs(vec![1, 2, 3]);
        }

        assert_eq!(recorder.frame_count(), 1);
        let data = recorder.finish();
        assert_eq!(data.frame_count(), 1);
        assert_eq!(data.header.map_name, "test_map");
    }

    #[test]
    fn replay_player_playback() {
        let mut recorder = ReplayRecorder::new("test_map");

        for i in 0..60 {
            let mut frame = recorder.record_frame(1.0 / 60.0);
            frame.add_entity(1, [i as f32, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]);
        }

        let data = recorder.finish();
        let mut player = ReplayPlayer::new(data);

        player.play();
        player.set_speed(1.0);

        let frame = player.update(0.5);
        assert!(frame.is_some());
        assert!(player.current_time() > 0.0);
    }

    #[test]
    fn replay_seek() {
        let mut recorder = ReplayRecorder::new("test_map");

        for _ in 0..60 {
            let _ = recorder.record_frame(1.0 / 60.0);
        }

        let data = recorder.finish();
        let mut player = ReplayPlayer::new(data);

        player.seek_to_frame(30);
        assert_eq!(player.current_frame(), 30);

        player.seek_to_percent(50.0);
        assert_eq!(player.progress_percent(), 50.0);
    }

    #[test]
    fn replay_serialization() {
        let mut recorder = ReplayRecorder::new("test_map");

        {
            let mut frame = recorder.record_frame(1.0 / 60.0);
            frame.add_entity(1, [0.0, 1.0, 2.0], [0.0, 0.0, 0.0, 1.0]);
        }

        let data = recorder.finish();
        let serialized = data.serialize().expect("Failed to serialize");
        let deserialized = ReplayData::deserialize(&serialized).expect("Failed to deserialize");

        assert_eq!(data.frame_count(), deserialized.frame_count());
        assert_eq!(data.header.map_name, deserialized.header.map_name);
    }

    #[test]
    fn replay_manager() {
        let mut manager = ReplayManager::new();

        assert!(!manager.is_recording());
        assert!(!manager.is_playing());

        manager.start_recording("test_map");
        assert!(manager.is_recording());

        let data = manager.stop_recording();
        assert!(data.is_some());
        assert!(!manager.is_recording());

        manager.start_playback(data.unwrap());
        assert!(!manager.is_playing());

        manager.player().unwrap().play();
        assert!(manager.is_playing());
    }
}
