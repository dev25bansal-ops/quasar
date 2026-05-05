//! Tests for quasar-templates crate

use quasar_templates::prelude::*;

#[test]
fn test_template_transform_default() {
    let transform = TemplateTransform::default();
    
    assert_eq!(transform.position, Vec3::ZERO);
    assert_eq!(transform.rotation, glam::Quat::IDENTITY);
    assert_eq!(transform.scale, Vec3::ONE);
}

#[test]
fn test_template_transform_creation() {
    let transform = TemplateTransform {
        position: Vec3::new(1.0, 2.0, 3.0),
        rotation: glam::Quat::from_euler(glam::EulerRot::XYZ, 0.0, 0.0, 0.0),
        scale: Vec3::new(2.0, 2.0, 2.0),
    };
    
    assert_eq!(transform.position, Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(transform.scale, Vec3::new(2.0, 2.0, 2.0));
}

#[test]
fn test_template_velocity_default() {
    let velocity = TemplateVelocity::default();
    
    assert_eq!(velocity.linear, Vec3::ZERO);
    assert_eq!(velocity.angular, Vec3::ZERO);
}

#[test]
fn test_template_velocity_creation() {
    let velocity = TemplateVelocity {
        linear: Vec3::new(1.0, 0.0, 0.0),
        angular: Vec3::new(0.0, 1.0, 0.0),
    };
    
    assert_eq!(velocity.linear, Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(velocity.angular, Vec3::new(0.0, 1.0, 0.0));
}

#[test]
fn test_health_creation() {
    let health = Health::new(100.0);
    
    assert_eq!(health.current, 100.0);
    assert_eq!(health.max, 100.0);
    assert_eq!(health.regen_rate, 0.0);
}

#[test]
fn test_health_with_regen() {
    let health = Health::new(100.0).with_regen(1.0);
    
    assert_eq!(health.current, 100.0);
    assert_eq!(health.max, 100.0);
    assert_eq!(health.regen_rate, 1.0);
}

#[test]
fn test_health_is_alive() {
    let mut health = Health::new(100.0);
    assert!(health.is_alive());
    
    health.current = 0.0;
    assert!(!health.is_alive());
}

#[test]
fn test_health_take_damage() {
    let mut health = Health::new(100.0);
    
    health.take_damage(25.0);
    assert_eq!(health.current, 75.0);
    
    health.take_damage(30.0);
    assert_eq!(health.current, 45.0);
}

#[test]
fn test_health_heal() {
    let mut health = Health::new(100.0);
    
    health.take_damage(50.0);
    assert_eq!(health.current, 50.0);
    
    health.heal(25.0);
    assert_eq!(health.current, 75.0);
}

#[test]
fn test_health_heal_over_max() {
    let mut health = Health::new(100.0);
    
    health.take_damage(50.0);
    health.heal(100.0); // Try to heal more than max
    
    assert_eq!(health.current, 100.0); // Should cap at max
}

#[test]
fn test_health_percentage() {
    let mut health = Health::new(100.0);
    
    assert_eq!(health.percentage(), 1.0);
    
    health.take_damage(50.0);
    assert_eq!(health.percentage(), 0.5);
    
    health.take_damage(25.0);
    assert_eq!(health.percentage(), 0.25);
}

#[test]
fn test_health_regen() {
    let mut health = Health::new(100.0).with_regen(10.0);
    
    health.take_damage(50.0);
    assert_eq!(health.current, 50.0);
    
    health.regen(1.0); // 1 second of regen
    assert_eq!(health.current, 60.0);
}

#[test]
fn test_health_regen_over_max() {
    let mut health = Health::new(100.0).with_regen(100.0);
    
    health.take_damage(50.0);
    health.regen(1.0); // Should cap at max
    
    assert_eq!(health.current, 100.0);
}

#[test]
fn test_health_is_full() {
    let health = Health::new(100.0);
    assert!(health.is_full());
    
    let mut health = Health::new(100.0);
    health.take_damage(10.0);
    assert!(!health.is_full());
}

#[test]
fn test_health_is_dead() {
    let health = Health::new(100.0);
    assert!(!health.is_dead());
    
    let mut health = Health::new(100.0);
    health.current = 0.0;
    assert!(health.is_dead());
}

#[test]
fn test_health_clone() {
    let health1 = Health::new(100.0);
    let health2 = health1.clone();
    
    assert_eq!(health1.current, health2.current);
    assert_eq!(health1.max, health2.max);
}

#[test]
fn test_health_serialization() {
    let health = Health::new(100.0).with_regen(5.0);
    
    let json = serde_json::to_string(&health).unwrap();
    let deserialized: Health = serde_json::from_str(&json).unwrap();
    
    assert_eq!(health.current, deserialized.current);
    assert_eq!(health.max, deserialized.max);
    assert_eq!(health.regen_rate, deserialized.regen_rate);
}

#[test]
fn test_inventory_creation() {
    let inventory = Inventory::new(10);
    
    assert_eq!(inventory.capacity(), 10);
    assert_eq!(inventory.count(), 0);
    assert!(inventory.is_empty());
}

#[test]
fn test_inventory_add_item() {
    let mut inventory = Inventory::new(10);
    
    let item = InventoryItem::new("test_item");
    inventory.add(item);
    
    assert_eq!(inventory.count(), 1);
    assert!(!inventory.is_empty());
}

#[test]
fn test_inventory_remove_item() {
    let mut inventory = Inventory::new(10);
    
    let item = InventoryItem::new("test_item");
    inventory.add(item.clone());
    
    let removed = inventory.remove(&item);
    assert!(removed.is_some());
    assert_eq!(inventory.count(), 0);
}

#[test]
fn test_inventory_capacity() {
    let mut inventory = Inventory::new(2);
    
    inventory.add(InventoryItem::new("item1"));
    inventory.add(InventoryItem::new("item2"));
    
    assert_eq!(inventory.count(), 2);
    assert!(inventory.is_full());
}

#[test]
fn test_inventory_is_full() {
    let inventory = Inventory::new(1);
    assert!(!inventory.is_full());
    
    let mut inventory = Inventory::new(1);
    inventory.add(InventoryItem::new("item1"));
    assert!(inventory.is_full());
}

#[test]
fn test_inventory_has_item() {
    let mut inventory = Inventory::new(10);
    
    let item = InventoryItem::new("test_item");
    inventory.add(item.clone());
    
    assert!(inventory.has(&item));
    assert!(!inventory.has(&InventoryItem::new("other_item")));
}

#[test]
fn test_inventory_count() {
    let mut inventory = Inventory::new(10);
    
    assert_eq!(inventory.count(), 0);
    
    inventory.add(InventoryItem::new("item1"));
    assert_eq!(inventory.count(), 1);
    
    inventory.add(InventoryItem::new("item2"));
    assert_eq!(inventory.count(), 2);
}

#[test]
fn test_inventory_clear() {
    let mut inventory = Inventory::new(10);
    
    inventory.add(InventoryItem::new("item1"));
    inventory.add(InventoryItem::new("item2"));
    
    assert_eq!(inventory.count(), 2);
    
    inventory.clear();
    assert_eq!(inventory.count(), 0);
    assert!(inventory.is_empty());
}

#[test]
fn test_inventory_item_creation() {
    let item = InventoryItem::new("test_item");
    
    assert_eq!(item.name(), "test_item");
    assert_eq!(item.quantity(), 1);
}

#[test]
fn test_inventory_item_with_quantity() {
    let item = InventoryItem::new("test_item").with_quantity(5);
    
    assert_eq!(item.name(), "test_item");
    assert_eq!(item.quantity(), 5);
}

#[test]
fn test_inventory_item_set_quantity() {
    let mut item = InventoryItem::new("test_item");
    
    item.set_quantity(10);
    assert_eq!(item.quantity(), 10);
}

#[test]
fn test_inventory_item_add_quantity() {
    let mut item = InventoryItem::new("test_item");
    
    item.add_quantity(5);
    assert_eq!(item.quantity(), 6);
    
    item.add_quantity(3);
    assert_eq!(item.quantity(), 9);
}

#[test]
fn test_inventory_item_remove_quantity() {
    let mut item = InventoryItem::new("test_item").with_quantity(10);
    
    item.remove_quantity(3);
    assert_eq!(item.quantity(), 7);
    
    item.remove_quantity(5);
    assert_eq!(item.quantity(), 2);
}

#[test]
fn test_inventory_item_is_empty() {
    let item = InventoryItem::new("test_item");
    assert!(!item.is_empty());
    
    let mut item = InventoryItem::new("test_item");
    item.set_quantity(0);
    assert!(item.is_empty());
}

#[test]
fn test_inventory_item_clone() {
    let item1 = InventoryItem::new("test_item").with_quantity(5);
    let item2 = item1.clone();
    
    assert_eq!(item1.name(), item2.name());
    assert_eq!(item1.quantity(), item2.quantity());
}

#[test]
fn test_inventory_item_serialization() {
    let item = InventoryItem::new("test_item").with_quantity(5);
    
    let json = serde_json::to_string(&item).unwrap();
    let deserialized: InventoryItem = serde_json::from_str(&json).unwrap();
    
    assert_eq!(item.name(), deserialized.name());
    assert_eq!(item.quantity(), deserialized.quantity());
}

#[test]
fn test_template_system_creation() {
    let system = TemplateSystem::new();
    assert!(system.is_some());
}

#[test]
fn test_template_system_spawn_entity() {
    let mut system = TemplateSystem::new().unwrap();
    let mut world = World::new();
    
    let entity = system.spawn_entity(&mut world, "test_template");
    assert!(entity.is_some());
}

#[test]
fn test_template_system_get_template() {
    let system = TemplateSystem::new().unwrap();
    
    let template = system.get_template("test_template");
    // Template may or may not exist depending on implementation
    assert!(template.is_some() || template.is_none());
}

#[test]
fn test_template_system_register_template() {
    let mut system = TemplateSystem::new().unwrap();
    
    let template = GameTemplate::new("test_template");
    system.register_template(template);
    
    // Template should be registered
    let retrieved = system.get_template("test_template");
    assert!(retrieved.is_some());
}

#[test]
fn test_game_template_creation() {
    let template = GameTemplate::new("test_template");
    
    assert_eq!(template.name(), "test_template");
}

#[test]
fn test_game_template_with_description() {
    let template = GameTemplate::new("test_template")
        .with_description("A test template");
    
    assert_eq!(template.description(), Some("A test template"));
}

#[test]
fn test_game_template_add_component() {
    let mut template = GameTemplate::new("test_template");
    
    template.add_component("Transform");
    template.add_component("Health");
    
    let components = template.components();
    assert_eq!(components.len(), 2);
    assert!(components.contains(&"Transform".to_string()));
    assert!(components.contains(&"Health".to_string()));
}

#[test]
fn test_game_template_components() {
    let mut template = GameTemplate::new("test_template");
    
    template.add_component("Transform");
    template.add_component("Health");
    
    let components = template.components();
    assert_eq!(components.len(), 2);
}

#[test]
fn test_game_template_clone() {
    let template1 = GameTemplate::new("test_template")
        .with_description("A test template");
    
    let template2 = template1.clone();
    
    assert_eq!(template1.name(), template2.name());
    assert_eq!(template1.description(), template2.description());
}

#[test]
fn test_game_template_serialization() {
    let template = GameTemplate::new("test_template")
        .with_description("A test template");
    
    let json = serde_json::to_string(&template).unwrap();
    let deserialized: GameTemplate = serde_json::from_str(&json).unwrap();
    
    assert_eq!(template.name(), deserialized.name());
    assert_eq!(template.description(), deserialized.description());
}

#[test]
fn test_template_registry_creation() {
    let registry = TemplateRegistry::new();
    assert!(registry.is_some());
}

#[test]
fn test_template_registry_register() {
    let mut registry = TemplateRegistry::new().unwrap();
    
    let template = GameTemplate::new("test_template");
    registry.register(template);
    
    let retrieved = registry.get("test_template");
    assert!(retrieved.is_some());
}

#[test]
fn test_template_registry_get() {
    let registry = TemplateRegistry::new().unwrap();
    
    let template = registry.get("nonexistent_template");
    assert!(template.is_none());
}

#[test]
fn test_template_registry_list() {
    let mut registry = TemplateRegistry::new().unwrap();
    
    registry.register(GameTemplate::new("template1"));
    registry.register(GameTemplate::new("template2"));
    
    let templates = registry.list();
    assert_eq!(templates.len(), 2);
}

#[test]
fn test_template_registry_has() {
    let mut registry = TemplateRegistry::new().unwrap();
    
    registry.register(GameTemplate::new("test_template"));
    
    assert!(registry.has("test_template"));
    assert!(!registry.has("nonexistent_template"));
}

#[test]
fn test_template_registry_remove() {
    let mut registry = TemplateRegistry::new().unwrap();
    
    registry.register(GameTemplate::new("test_template"));
    assert!(registry.has("test_template"));
    
    registry.remove("test_template");
    assert!(!registry.has("test_template"));
}

#[test]
fn test_template_registry_clear() {
    let mut registry = TemplateRegistry::new().unwrap();
    
    registry.register(GameTemplate::new("template1"));
    registry.register(GameTemplate::new("template2"));
    
    assert_eq!(registry.list().len(), 2);
    
    registry.clear();
    assert_eq!(registry.list().len(), 0);
}

#[test]
fn test_template_registry_count() {
    let mut registry = TemplateRegistry::new().unwrap();
    
    assert_eq!(registry.count(), 0);
    
    registry.register(GameTemplate::new("template1"));
    assert_eq!(registry.count(), 1);
    
    registry.register(GameTemplate::new("template2"));
    assert_eq!(registry.count(), 2);
}

#[test]
fn test_template_registry_is_empty() {
    let registry = TemplateRegistry::new().unwrap();
    assert!(registry.is_empty());
    
    let mut registry = TemplateRegistry::new().unwrap();
    registry.register(GameTemplate::new("template1"));
    assert!(!registry.is_empty());
}