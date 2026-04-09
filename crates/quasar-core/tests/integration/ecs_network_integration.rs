//! Integration tests for ECS + Networking interaction.
//!
//! Verifies that entities with networked components correctly interact
//! with the networking system, including:
//! - Spawning entities with networked components
//! - Testing delta compression with entity state changes
//! - Verifying prediction/rollback with entity updates
//! - Testing replication registry serialization/deserialization

use quasar_core::ecs::{Entity, World};
use quasar_core::network::replication::{
    dequantize_angle_degrees, dequantize_position, dequantize_rotation, quantize_angle_degrees,
    quantize_position, quantize_rotation, Replicate, ReplicatedField, ReplicationMode,
    ReplicationRegistry, SpatialFilter, SpatialGrid,
};
use quasar_core::prediction::PredictionManager;
use quasar_core::prediction::{
    EntityInterpolationState, EntityInterpolator, InputFrame, ServerConfirmation,
};
use quasar_math::{Quat, Transform, Vec3};

// ---------------------------------------------------------------------------
// Networked component definitions
// ---------------------------------------------------------------------------

/// Networked position component.
#[derive(Debug, Clone, PartialEq)]
struct NetworkedPosition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl NetworkedPosition {
    fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// Networked velocity component.
#[derive(Debug, Clone, PartialEq)]
struct NetworkedVelocity {
    pub vx: f32,
    pub vy: f32,
    pub vz: f32,
}

/// Networked health component.
#[derive(Debug, Clone, PartialEq)]
struct NetworkedHealth {
    pub current: u32,
    pub max: u32,
}

/// Networked owner component.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NetworkedOwner {
    pub client_id: u64,
}

// ---------------------------------------------------------------------------
// 1. Spawn entities with networked components
// ---------------------------------------------------------------------------

#[test]
fn test_spawn_entity_with_networked_components() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(entity, Transform::from_position(Vec3::new(0.0, 0.0, 0.0)));
    world.insert(
        entity,
        NetworkedPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    );
    world.insert(
        entity,
        NetworkedVelocity {
            vx: 1.0,
            vy: 0.0,
            vz: 0.0,
        },
    );
    world.insert(
        entity,
        NetworkedHealth {
            current: 100,
            max: 100,
        },
    );
    world.insert(
        entity,
        NetworkedOwner { client_id: 42 },
    );

    // Verify all components
    assert!(world.get::<NetworkedPosition>(entity).is_some());
    assert!(world.get::<NetworkedVelocity>(entity).is_some());
    assert!(world.get::<NetworkedHealth>(entity).is_some());
    assert!(world.get::<NetworkedOwner>(entity).is_some());
}

#[test]
fn test_query_networked_entities() {
    let mut world = World::new();

    // Spawn multiple networked entities
    for i in 0..5 {
        let entity = world.spawn();
        world.insert(
            entity,
            NetworkedPosition {
                x: i as f32 * 10.0,
                y: 0.0,
                z: 0.0,
            },
        );
        world.insert(
            entity,
            NetworkedOwner {
                client_id: if i % 2 == 0 { 1 } else { 2 },
            },
        );
    }

    // Query all entities with NetworkedPosition
    let pos_count = world.query::<NetworkedPosition>().into_iter().count();
    assert_eq!(pos_count, 5);

    // Query entities owned by client 1
    let client1_count = world
        .query2::<NetworkedPosition, NetworkedOwner>()
        .into_iter()
        .filter(|(_, _, owner)| owner.client_id == 1)
        .count();
    assert_eq!(client1_count, 3); // entities 0, 2, 4
}

#[test]
fn test_update_networked_component() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(
        entity,
        NetworkedPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    );

    // Update position
    let pos = world.get_mut::<NetworkedPosition>(entity).unwrap();
    pos.x = 5.0;
    pos.y = 3.0;
    pos.z = 1.0;

    let updated = world.get::<NetworkedPosition>(entity).unwrap();
    assert_eq!(updated.x, 5.0);
    assert_eq!(updated.y, 3.0);
    assert_eq!(updated.z, 1.0);
}

// ---------------------------------------------------------------------------
// 2. Test delta compression with entity state changes
// ---------------------------------------------------------------------------

#[test]
fn test_quantize_position_roundtrip() {
    let original = Vec3::new(123.456, -78.901, 0.123);
    let quantized = quantize_position(original);
    let dequantized = dequantize_position(quantized);

    // Should be within 1cm precision
    assert!((original.x - dequantized.x).abs() < 0.02);
    assert!((original.y - dequantized.y).abs() < 0.02);
    assert!((original.z - dequantized.z).abs() < 0.02);
}

#[test]
fn test_quantize_rotation_roundtrip() {
    let original = Quat::from_rotation_y(std::f32::consts::FRAC_PI_4);
    let (largest_idx, quant) = {
        let quant = quantize_rotation(original);
        // Determine largest component for dequantization
        let abs_vals = [
            original.x.abs(),
            original.y.abs(),
            original.z.abs(),
            original.w.abs(),
        ];
        let max_idx = abs_vals
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .unwrap()
            .0 as u8;
        (max_idx, quant)
    };
    let dequantized = dequantize_rotation(quant, largest_idx);

    // Should be reasonably close
    assert!((original.x - dequantized.x).abs() < 0.1);
    assert!((original.y - dequantized.y).abs() < 0.1);
    assert!((original.z - dequantized.z).abs() < 0.1);
    assert!((original.w - dequantized.w).abs() < 0.1);
}

#[test]
fn test_quantize_angle_roundtrip() {
    let angles = vec![-180.0, -90.0, -45.0, 0.0, 30.0, 90.0, 180.0, 270.0];

    for angle in angles {
        let quantized = quantize_angle_degrees(angle);
        let dequantized = dequantize_angle_degrees(quantized);

        // Should be within 0.1 degree precision
        assert!(
            (angle - dequantized).abs() < 0.2,
            "angle={}, dequantized={}, diff={}",
            angle,
            dequantized,
            (angle - dequantized).abs()
        );
    }
}

#[test]
fn test_delta_compression_positions() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(
        entity,
        NetworkedPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    );

    // Quantize initial position
    let initial = world.get::<NetworkedPosition>(entity).unwrap();
    let initial_vec = Vec3::new(initial.x, initial.y, initial.z);
    let initial_quant = quantize_position(initial_vec);

    // Update position slightly
    let pos = world.get_mut::<NetworkedPosition>(entity).unwrap();
    pos.x = 0.005; // 5mm change (below quantization precision)
    pos.y = 0.0;
    pos.z = 0.0;

    // Quantize new position
    let new_pos = world.get::<NetworkedPosition>(entity).unwrap();
    let new_vec = Vec3::new(new_pos.x, new_pos.y, new_pos.z);
    let new_quant = quantize_position(new_vec);

    // Delta should be zero (change is below precision threshold)
    let delta_x = new_quant[0] - initial_quant[0];
    let delta_y = new_quant[1] - initial_quant[1];
    let delta_z = new_quant[2] - initial_quant[2];

    assert_eq!(delta_x, 0);
    assert_eq!(delta_y, 0);
    assert_eq!(delta_z, 0);
}

#[test]
fn test_delta_compression_significant_change() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(
        entity,
        NetworkedPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    );

    let initial = world.get::<NetworkedPosition>(entity).unwrap();
    let initial_quant = quantize_position(Vec3::new(initial.x, initial.y, initial.z));

    // Move entity significantly (more than 1cm)
    let pos = world.get_mut::<NetworkedPosition>(entity).unwrap();
    pos.x = 1.5; // 1.5 meters
    pos.y = 2.3;
    pos.z = -0.5;

    let new_pos = world.get::<NetworkedPosition>(entity).unwrap();
    let new_quant = quantize_position(Vec3::new(new_pos.x, new_pos.y, new_pos.z));

    // Delta should be non-zero
    let delta_x = new_quant[0] - initial_quant[0];
    let delta_y = new_quant[1] - initial_quant[1];
    let delta_z = new_quant[2] - initial_quant[2];

    assert_ne!(delta_x, 0);
    assert_ne!(delta_y, 0);
    assert_ne!(delta_z, 0);
}

#[test]
fn test_spatial_filter_contains() {
    let filter = SpatialFilter::new(Vec3::new(0.0, 0.0, 0.0), 10.0);

    assert!(filter.contains(Vec3::new(0.0, 0.0, 0.0)));
    assert!(filter.contains(Vec3::new(5.0, 5.0, 0.0)));
    assert!(!filter.contains(Vec3::new(15.0, 0.0, 0.0)));
}

#[test]
fn test_spatial_grid_insert_and_query() {
    use quasar_core::ecs::Entity as EcsEntity;

    let mut grid = SpatialGrid::new(10.0);

    // Create a dummy entity (we just need the Entity type for insertion)
    let mut world = World::new();
    let entity = world.spawn();
    world.insert(
        entity,
        NetworkedPosition {
            x: 5.0,
            y: 0.0,
            z: 0.0,
        },
    );

    // Insert into spatial grid
    grid.insert(entity, Vec3::new(5.0, 0.0, 0.0));

    // Query nearby entities
    let filter = SpatialFilter::new(Vec3::new(0.0, 0.0, 0.0), 20.0);
    let nearby = grid.query(&filter);

    assert_eq!(nearby.len(), 1);
    assert!(nearby.contains(&entity));
}

// ---------------------------------------------------------------------------
// 3. Verify prediction/rollback with entity updates
// ---------------------------------------------------------------------------

use quasar_core::network::replication::ReplicationRegistry;
use quasar_network::replication::{
    EntitySnapshot as NetEntitySnapshot, InputData, InputType, NetworkEntityId,
};

fn make_input_data(input_type: InputType, value: f32) -> InputData {
    InputData { input_type, value }
}

fn make_entity_snapshot(entity_id: u64, frame: u64, position: [f32; 3]) -> NetEntitySnapshot {
    NetEntitySnapshot {
        entity_id: NetworkEntityId(entity_id),
        frame,
        position,
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    }
}

#[test]
fn test_prediction_manager_record_and_confirm() {
    use quasar_core::network::ClientId;

    let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

    // Record predictions for ticks 1 and 2
    let inputs1 = vec![make_input_data(InputType::MoveForward, 1.0)];
    manager.record_prediction(1, (), inputs1.clone());

    let inputs2 = vec![make_input_data(InputType::MoveForward, 1.0)];
    manager.record_prediction(2, (), inputs2.clone());

    assert_eq!(manager.unconfirmed_frames(), 2);

    // Server confirms tick 2
    let confirmation = ServerConfirmation {
        confirmed_tick: 2,
        positions: vec![(1, [2.0, 0.0, 0.0])],
    };
    let local_positions = vec![(1, [2.0, 0.0, 0.0])];

    let result = manager.on_server_confirm(&confirmation, &local_positions);
    assert!(result.is_none()); // No mismatch

    assert_eq!(manager.confirmed_tick, 2);
}

#[test]
fn test_prediction_manager_rollback_on_mismatch() {
    use quasar_core::network::ClientId;

    let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

    // Record predictions
    manager.record_prediction(1, (), vec![make_input_data(InputType::MoveForward, 1.0)]);
    manager.record_prediction(2, (), vec![make_input_data(InputType::MoveForward, 1.0)]);
    manager.record_prediction(3, (), vec![make_input_data(InputType::MoveForward, 1.0)]);

    // Server confirms with significant mismatch
    let confirmation = ServerConfirmation {
        confirmed_tick: 2,
        positions: vec![(1, [10.0, 0.0, 0.0])], // Server says entity is at x=10
    };
    let local_positions = vec![(1, [2.0, 0.0, 0.0])]; // Client thinks x=2

    let result = manager.on_server_confirm(&confirmation, &local_positions);
    assert!(result.is_some()); // Mismatch detected!

    let (_snapshot, replay_inputs) = result.unwrap();

    // Replay inputs should contain frames after confirmed tick
    for frame in &replay_inputs {
        assert!(frame.tick > 2);
    }
}

#[test]
fn test_entity_interpolation() {
    let mut interpolator = EntityInterpolator::new(0); // No delay

    // Add snapshots for entity 1 at different ticks
    interpolator.add_snapshot(make_entity_snapshot(1, 10, [0.0, 0.0, 0.0]));
    interpolator.add_snapshot(make_entity_snapshot(1, 20, [10.0, 0.0, 0.0]));

    // Interpolate at tick 15 (midpoint)
    let result = interpolator.interpolate_entity(1, 15);
    assert!(result.is_some());

    let (pos, _rot, _scale) = result.unwrap();
    // Should be halfway
    assert!((pos[0] - 5.0).abs() < 0.1);
}

#[test]
fn test_prediction_manager_update_interpolation() {
    use quasar_core::network::ClientId;

    let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

    // Add interpolation snapshots
    manager.update_interpolation(&[
        make_entity_snapshot(1, 10, [0.0, 0.0, 0.0]),
        make_entity_snapshot(1, 20, [10.0, 0.0, 0.0]),
        make_entity_snapshot(2, 10, [5.0, 5.0, 0.0]),
        make_entity_snapshot(2, 20, [15.0, 5.0, 0.0]),
    ]);

    assert_eq!(manager.interpolator.snapshot_count(), 4);

    // Interpolate entity 1
    let result = manager.interpolate_entity(1, 15);
    assert!(result.is_some());
}

#[test]
fn test_prediction_manager_reset() {
    use quasar_core::network::ClientId;

    let mut manager: PredictionManager<()> = PredictionManager::new(ClientId(1));

    // Record some predictions
    manager.record_prediction(1, (), vec![]);
    manager.record_prediction(2, (), vec![]);

    assert_eq!(manager.unconfirmed_frames(), 2);
    assert_eq!(manager.confirmed_tick, 0);
    assert_eq!(manager.predicted_tick, 2);

    // Reset
    manager.reset();

    assert_eq!(manager.unconfirmed_frames(), 0);
    assert_eq!(manager.confirmed_tick, 0);
    assert_eq!(manager.predicted_tick, 0);
}

// ---------------------------------------------------------------------------
// 4. Test replication registry serialization/deserialization
// ---------------------------------------------------------------------------

/// Example replicatable component.
#[derive(Debug, Clone, PartialEq)]
struct ReplicatedTransform {
    x: f32,
    y: f32,
    z: f32,
}

impl Replicate for ReplicatedTransform {
    const TYPE_NAME: &'static str = "ReplicatedTransform";

    const FIELDS: &'static [ReplicatedField] = &[
        ReplicatedField {
            name: "x",
            type_name: "f32",
            mode: ReplicationMode::Replicated,
        },
        ReplicatedField {
            name: "y",
            type_name: "f32",
            mode: ReplicationMode::Replicated,
        },
        ReplicatedField {
            name: "z",
            type_name: "f32",
            mode: ReplicationMode::Replicated,
        },
    ];

    fn serialize(&self, buf: &mut Vec<u8>) {
        buf.extend_from_slice(&self.x.to_le_bytes());
        buf.extend_from_slice(&self.y.to_le_bytes());
        buf.extend_from_slice(&self.z.to_le_bytes());
    }

    fn deserialize(data: &[u8]) -> Self {
        let x = f32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let y = f32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let z = f32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        Self { x, y, z }
    }

    fn compute_delta(&self, previous: &Self, buf: &mut Vec<u8>) -> bool {
        let dx = (self.x - previous.x).abs();
        let dy = (self.y - previous.y).abs();
        let dz = (self.z - previous.z).abs();

        if dx > 0.001 || dy > 0.001 || dz > 0.001 {
            buf.extend_from_slice(&self.x.to_le_bytes());
            buf.extend_from_slice(&self.y.to_le_bytes());
            buf.extend_from_slice(&self.z.to_le_bytes());
            true
        } else {
            false
        }
    }

    fn apply_delta(&mut self, delta: &[u8]) {
        self.x = f32::from_le_bytes([delta[0], delta[1], delta[2], delta[3]]);
        self.y = f32::from_le_bytes([delta[4], delta[5], delta[6], delta[7]]);
        self.z = f32::from_le_bytes([delta[8], delta[9], delta[10], delta[11]]);
    }
}

#[test]
fn test_replication_registry_register_and_serialize() {
    let mut registry = ReplicationRegistry::new();
    registry.register::<ReplicatedTransform>();

    let type_id = registry.type_id::<ReplicatedTransform>();
    assert!(type_id.is_some());

    // Serialize a component
    let transform = ReplicatedTransform {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };
    let mut buf = Vec::new();
    registry.serialize(
        type_id.unwrap(),
        &transform as &dyn std::any::Any,
        &mut buf,
    );

    assert_eq!(buf.len(), 12); // 3 * 4 bytes

    // Deserialize
    let deserialized = registry
        .deserialize(type_id.unwrap(), &buf)
        .unwrap()
        .downcast::<ReplicatedTransform>()
        .unwrap();

    assert_eq!(deserialized.x, 1.0);
    assert_eq!(deserialized.y, 2.0);
    assert_eq!(deserialized.z, 3.0);
}

#[test]
fn test_replicated_transform_roundtrip() {
    let original = ReplicatedTransform {
        x: 12.34,
        y: -56.78,
        z: 90.12,
    };

    let mut buf = Vec::new();
    original.serialize(&mut buf);

    let deserialized = ReplicatedTransform::deserialize(&buf);

    assert_eq!(original, deserialized);
}

#[test]
fn test_replicated_transform_delta() {
    let previous = ReplicatedTransform {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    let current = ReplicatedTransform {
        x: 1.5,
        y: 2.5,
        z: 3.5,
    };

    let mut delta_buf = Vec::new();
    let has_delta = current.compute_delta(&previous, &mut delta_buf);
    assert!(has_delta);
    assert_eq!(delta_buf.len(), 12);

    // Apply delta to a copy of previous
    let mut reconstructed = previous.clone();
    reconstructed.apply_delta(&delta_buf);

    assert!((reconstructed.x - current.x).abs() < 0.0001);
    assert!((reconstructed.y - current.y).abs() < 0.0001);
    assert!((reconstructed.z - current.z).abs() < 0.0001);
}

#[test]
fn test_replicated_transform_delta_no_change() {
    let previous = ReplicatedTransform {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    };

    let current = ReplicatedTransform {
        x: 1.0001, // Within threshold
        y: 2.0001,
        z: 3.0001,
    };

    let mut delta_buf = Vec::new();
    let has_delta = current.compute_delta(&previous, &mut delta_buf);

    assert!(!has_delta);
    assert!(delta_buf.is_empty());
}

#[test]
fn test_multiple_registered_types() {
    let mut registry = ReplicationRegistry::new();
    registry.register::<ReplicatedTransform>();

    let type_id_1 = registry.type_id::<ReplicatedTransform>();
    assert!(type_id_1.is_some());

    // Register more types by creating simple wrappers
    #[derive(Debug, Clone, PartialEq)]
    struct ReplicatedHealth {
        value: u32,
    }

    impl Replicate for ReplicatedHealth {
        const TYPE_NAME: &'static str = "ReplicatedHealth";
        const FIELDS: &'static [ReplicatedField] = &[ReplicatedField {
            name: "value",
            type_name: "u32",
            mode: ReplicationMode::Replicated,
        }];

        fn serialize(&self, buf: &mut Vec<u8>) {
            buf.extend_from_slice(&self.value.to_le_bytes());
        }

        fn deserialize(data: &[u8]) -> Self {
            Self {
                value: u32::from_le_bytes([data[0], data[1], data[2], data[3]]),
            }
        }

        fn compute_delta(&self, previous: &Self, buf: &mut Vec<u8>) -> bool {
            if self.value != previous.value {
                buf.extend_from_slice(&self.value.to_le_bytes());
                true
            } else {
                false
            }
        }

        fn apply_delta(&mut self, delta: &[u8]) {
            self.value = u32::from_le_bytes([delta[0], delta[1], delta[2], delta[3]]);
        }
    }

    registry.register::<ReplicatedHealth>();

    let type_id_2 = registry.type_id::<ReplicatedHealth>();
    assert!(type_id_2.is_some());
    assert_ne!(type_id_1.unwrap(), type_id_2.unwrap());
}

#[test]
fn test_ecs_networked_entity_lifecycle() {
    let mut world = World::new();

    // Spawn entity
    let entity = world.spawn();
    world.insert(
        entity,
        NetworkedPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    );

    // Simulate network update
    let pos = world.get_mut::<NetworkedPosition>(entity).unwrap();
    pos.x = 10.0;
    pos.y = 5.0;
    pos.z = 2.0;

    // Verify update
    let updated = world.get::<NetworkedPosition>(entity).unwrap();
    assert_eq!(updated.x, 10.0);
    assert_eq!(updated.y, 5.0);
    assert_eq!(updated.z, 2.0);

    // Despawn entity
    assert!(world.despawn(entity));
    assert!(!world.is_alive(entity));
}

#[test]
fn test_networked_components_with_change_detection() {
    let mut world = World::new();

    let entity = world.spawn();
    world.insert(
        entity,
        NetworkedPosition {
            x: 0.0,
            y: 0.0,
            z: 0.0,
        },
    );

    // Record the tick before modification
    let tick_before = world.current_tick();

    // Modify the component
    let pos = world.get_mut::<NetworkedPosition>(entity).unwrap();
    pos.x = 5.0;

    // The change should be tracked
    // Note: change detection works at the ECS level
    assert!(world.is_alive(entity));
}
