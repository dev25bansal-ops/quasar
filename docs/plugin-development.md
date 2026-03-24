# Plugin Development

Plugins extend Quasar's functionality by organizing related systems, resources, and components.

## Plugin Trait

```rust,ignore
use quasar_core::{App, Plugin, System, SystemStage, World};

pub struct MyPlugin;

impl Plugin for MyPlugin {
    fn name(&self) -> &str {
        "my_plugin"
    }

    fn build(&self, app: &mut App) {
        // Add resources
        app.world.insert_resource(MyResource::default());

        // Add systems
        app.schedule.add_system(SystemStage::Update, Box::new(MySystem));

        // Register components
        // Components are auto-registered when used
    }

    fn dependencies(&self) -> &[&'static str] {
        &["transform", "render"]
    }
}
```

## Creating a Plugin

### Basic Structure

```
my_plugin/
├── Cargo.toml
├── src/
│   ├── lib.rs          # Plugin definition
│   ├── components.rs   # Component types
│   ├── resources.rs    # Resource types
│   ├── systems.rs      # System implementations
│   └── tests.rs        # Plugin tests
```

### Cargo.toml

```toml
[package]
name = "quasar-my-plugin"
version = "0.1.0"
edition = "2021"

[dependencies]
quasar-core = { path = "../quasar-core" }
quasar-math = { path = "../quasar-math" }

[dev-dependencies]
quasar-engine = { path = "../quasar-engine" }
```

### lib.rs

```rust,ignore
mod components;
mod resources;
mod systems;

use quasar_core::{App, Plugin, SystemStage};

pub use components::*;
pub use resources::*;
pub use systems::*;

pub struct MyPlugin {
    pub config: MyConfig,
}

impl Plugin for MyPlugin {
    fn name(&self) -> &str {
        "my_plugin"
    }

    fn build(&self, app: &mut App) {
        // Insert configuration
        app.world.insert_resource(MyResource::new(self.config.clone()));

        // Add systems in order
        app.schedule.add_system(SystemStage::PreUpdate, Box::new(SetupSystem));
        app.schedule.add_system(SystemStage::Update, Box::new(LogicSystem));
        app.schedule.add_system(SystemStage::PostUpdate, Box::new(CleanupSystem));
    }
}

#[derive(Clone)]
pub struct MyConfig {
    pub enabled: bool,
    pub value: f32,
}

impl Default for MyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            value: 1.0,
        }
    }
}
```

## Systems

### System Trait

```rust,ignore
use quasar_core::{System, World, SystemStage};

pub struct MySystem;

impl System for MySystem {
    fn name(&self) -> &str {
        "my_system"
    }

    fn run(&mut self, world: &mut World) {
        // Access resources
        let resource = world.resource::<MyResource>()
            .expect("MyResource not found");

        // Query entities
        for (entity, component) in world.query_iter::<MyComponent>() {
            // Process
        }

        // Mutate components
        for (entity, component) in world.query_iter_mut::<MyComponent>() {
            component.value += 1.0;
        }
    }
}
```

### System with Resources

```rust,ignore
pub struct PhysicsSystem;

impl System for PhysicsSystem {
    fn name(&self) -> &str {
        "physics"
    }

    fn run(&mut self, world: &mut World) {
        let physics = world.resource_mut::<PhysicsWorld>();
        let dt = world.resource::<Time>().delta_seconds();

        physics.step(dt);

        // Sync transforms
        for (entity, body) in world.query_iter::<RigidBody>() {
            if let Some(transform) = world.get_mut::<Transform>(entity) {
                transform.position = body.position;
            }
        }
    }
}
```

### System Ordering

```rust,ignore
impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) {
        // Systems run in order they are added
        app.schedule.add_system(SystemStage::Update, Box::new(InputSystem));
        app.schedule.add_system(SystemStage::Update, Box::new(LogicSystem));
        app.schedule.add_system(SystemStage::Update, Box::new(OutputSystem));
    }
}
```

## Resources

### Defining Resources

```rust,ignore
#[derive(Debug, Clone)]
pub struct MyResource {
    pub data: Vec<f32>,
    pub config: MyConfig,
}

impl MyResource {
    pub fn new(config: MyConfig) -> Self {
        Self {
            data: Vec::new(),
            config,
        }
    }
}
```

### Accessing Resources

```rust,ignore
// Read-only access
if let Some(resource) = world.resource::<MyResource>() {
    println!("Data: {:?}", resource.data);
}

// Mutable access
if let Some(resource) = world.resource_mut::<MyResource>() {
    resource.data.push(1.0);
}
```

## Components

### Defining Components

```rust,ignore
use quasar_core::Component;

#[derive(Component, Clone)]
pub struct MyComponent {
    pub value: f32,
    pub enabled: bool,
}

#[derive(Component, Clone, Copy)]
pub struct MyTag;  // Zero-sized tag component
```

### Component Registration

Components are automatically registered when first used:

```rust,ignore
// This registers the component
let entity = world.spawn();
world.insert(entity, MyComponent { value: 1.0, enabled: true });
```

## Events

### Defining Events

```rust,ignore
#[derive(Debug, Clone)]
pub struct MyEvent {
    pub entity: Entity,
    pub data: f32,
}
```

### Sending Events

```rust,ignore
impl System for EventSystem {
    fn run(&mut self, world: &mut World) {
        if let Some(events) = world.resource_mut::<Events>() {
            events.send(MyEvent {
                entity: some_entity,
                data: 42.0,
            });
        }
    }
}
```

### Reading Events

```rust,ignore
impl System for EventHandler {
    fn run(&mut self, world: &mut World) {
        if let Some(events) = world.resource::<Events>() {
            for event in events.read::<MyEvent>() {
                println!("Event: {:?}", event);
            }
            events.clear::<MyEvent>();
        }
    }
}
```

## Testing

### Unit Tests

```rust,ignore
#[cfg(test)]
mod tests {
    use super::*;
    use quasar_core::World;

    #[test]
    fn test_my_system() {
        let mut world = World::new();
        world.insert_resource(MyResource::default());

        let entity = world.spawn();
        world.insert(entity, MyComponent { value: 0.0, enabled: true });

        let mut system = MySystem;
        system.run(&mut world);

        let components = world.query::<MyComponent>();
        assert_eq!(components.len(), 1);
    }
}
```

### Integration Tests

```rust,ignore
#[cfg(test)]
mod tests {
    use super::*;
    use quasar_engine::App;

    #[test]
    fn test_plugin_integration() {
        let mut app = App::new();
        app.add_plugin(MyPlugin::default());

        // Verify resources
        assert!(app.world.resource::<MyResource>().is_some());

        // Run one frame
        app.update();
    }
}
```

## Best Practices

### 1. Single Responsibility

```rust,ignore
// Good - each plugin has one job
pub struct PhysicsPlugin;
pub struct AudioPlugin;
pub struct InputPlugin;

// Bad - god plugin
pub struct GamePlugin;  // Does everything
```

### 2. Document Public APIs

````rust,ignore
/// My plugin does X.
///
/// # Example
/// ```
/// let mut app = App::new();
/// app.add_plugin(MyPlugin::default());
/// ```
pub struct MyPlugin;

/// My component stores Y.
#[derive(Component, Clone)]
pub struct MyComponent {
    /// The value of the thing.
    pub value: f32,
}
````

### 3. Use Type-State Pattern

```rust,ignore
pub struct MyPlugin<STATE> {
    state: STATE,
}

pub struct Uninitialized;
pub struct Initialized { resource: MyResource };

impl MyPlugin<Uninitialized> {
    pub fn new() -> Self {
        Self { state: Uninitialized }
    }

    pub fn with_config(self, config: MyConfig) -> Self {
        self
    }

    pub fn build(self, app: &mut App) -> MyPlugin<Initialized> {
        // Build plugin
        MyPlugin { state: Initialized { resource: MyResource::default() } }
    }
}
```

### 4. Handle Missing Dependencies

```rust,ignore
impl Plugin for MyPlugin {
    fn build(&self, app: &mut App) {
        if app.world.resource::<DependencyResource>().is_none() {
            log::warn!("MyPlugin requires DependencyPlugin. Adding default.");
            app.add_plugin(DependencyPlugin::default());
        }

        // Continue building
    }
}
```

## Advanced Patterns

### Plugin Groups

```rust,ignore
pub struct StandardPlugins;

impl Plugin for StandardPlugins {
    fn name(&self) -> &str { "standard_plugins" }

    fn build(&self, app: &mut App) {
        app.add_plugin(TransformPlugin)
            .add_plugin(PhysicsPlugin)
            .add_plugin(RenderPlugin)
            .add_plugin(AudioPlugin);
    }
}
```

### Conditional Systems

```rust,ignore
pub struct DebugPlugin {
    pub enabled: bool,
}

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        if self.enabled {
            app.schedule.add_system(SystemStage::Update, Box::new(DebugRenderSystem));
        }
    }
}
```

### Resource Dependencies

```rust,ignore
impl Plugin for RenderPlugin {
    fn dependencies(&self) -> &[&'static str] {
        &["transform", "window"]
    }

    fn build(&self, app: &mut App) {
        // Transform and Window plugins must be loaded
        assert!(app.world.resource::<TransformResource>().is_some());
        assert!(app.world.resource::<Window>().is_some());

        // Build render plugin
    }
}
```

## Publishing

### Cargo.toml

```toml
[package]
name = "quasar-my-plugin"
version = "0.1.0"
edition = "2021"
license = "MIT"
description = "A plugin for Quasar Engine"
repository = "https://github.com/user/quasar-my-plugin"
keywords = ["game", "engine", "quasar", "plugin"]
categories = ["game-engines"]

[dependencies]
quasar-core = "0.1"
```

### README.md

````markdown
# quasar-my-plugin

A plugin for Quasar Engine that does X.

## Usage

```rust
use quasar_engine::prelude::*;
use quasar_my_plugin::MyPlugin;

fn main() {
    App::new()
        .add_plugin(MyPlugin::default())
        .run();
}
```
````

## Features

- Feature 1
- Feature 2

```

## Next Steps

- [Architecture](../architecture.md)
- [Scripting API](../scripting/scripting-api.md)
- [Examples](../examples/)
```
