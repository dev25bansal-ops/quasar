//! Engine integration tests

#[test]
fn prelude_exports() {
    // Verify all expected types are exported
    use quasar_engine::prelude::*;

    // Core types
    let _ = Vec3::ZERO;
    let _ = Quat::IDENTITY;
    let _ = Transform::IDENTITY;
}

#[test]
fn app_creation() {
    use quasar_engine::prelude::*;

    let app = App::new();
    assert_eq!(app.world.entity_count(), 0);
}

#[test]
fn world_spawn() {
    use quasar_engine::prelude::*;

    let mut world = World::new();
    let entity = world.spawn();
    assert!(world.is_alive(entity));
}

#[test]
fn world_insert_component() {
    use quasar_engine::prelude::*;

    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);

    let transform = world.get::<Transform>(entity);
    assert!(transform.is_some());
}

#[test]
fn world_remove_component() {
    use quasar_engine::prelude::*;

    let mut world = World::new();
    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.remove::<Transform>(entity);

    let transform = world.get::<Transform>(entity);
    assert!(transform.is_none());
}

#[test]
fn world_despawn() {
    use quasar_engine::prelude::*;

    let mut world = World::new();
    let entity = world.spawn();
    world.despawn(entity);

    assert!(!world.is_alive(entity));
}

#[test]
fn time_default() {
    use quasar_engine::prelude::*;

    let time = Time::new();
    assert!(time.delta_seconds() >= 0.0);
}

#[test]
fn events_creation() {
    use quasar_engine::prelude::*;

    let events: Events<String> = Events::new();
    events.send("test".to_string());

    let reader = events.read();
    assert_eq!(reader.count(), 1);
}

#[test]
fn plugin_registration() {
    use quasar_engine::prelude::*;

    struct TestPlugin;

    impl Plugin for TestPlugin {
        fn build(&self, _app: &mut App) {}
    }

    let mut app = App::new();
    app.add_plugin(TestPlugin);
}

#[test]
fn system_registration() {
    use quasar_engine::prelude::*;

    fn test_system(_world: &mut World) {}

    let mut app = App::new();
    app.add_system("test", test_system);
}
