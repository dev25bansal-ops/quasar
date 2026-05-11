//! Integration tests for the public quasar-templates API.

use glam::{Vec2 as GlamVec2, Vec3};
use quasar_core::ecs::{System, World};
use quasar_core::Plugin;
use quasar_templates::fps::{
    Ammo, EnemyAI, EnemyAISystem, EnemyState, FpsMovementSystem, FpsPlayer, FpsPlugin, Weapon,
    WeaponSystem, WeaponType,
};
use quasar_templates::platformer::{
    Collectible, CollectibleType, PlatformerPlayer, PlayerInventory,
};
use quasar_templates::rpg::{
    Ability, CharacterStats, Equipment, EquipmentSlot, Inventory, InventoryItem, ItemType,
    ObjectiveType, Quest, QuestObjective, QuestState, RpgPlayer, RpgPlugin, RpgQuestSystem,
    RpgSkillSystem, Skill, SkillType, StatType,
};
use quasar_templates::rts::{
    FogOfWar, PlayerResources, ResourceCarrying, ResourceCost, ResourceGenerator, ResourceType,
    SelectionManager,
};
use quasar_templates::{
    spawn_entity, spawn_entity_at, Health, InputState, Team, TemplateTransform, TemplateVelocity,
    Timer, Vec2 as TemplateVec2,
};

#[test]
fn common_components_track_health_timer_and_transform_state() {
    let mut health = Health::new(100.0);
    assert!(health.is_alive());
    assert!(!health.take_damage(25.0));
    assert_eq!(health.current, 75.0);
    health.heal(200.0);
    assert_eq!(health.current, health.max);

    health.invulnerable = true;
    assert!(!health.take_damage(50.0));
    assert_eq!(health.current, health.max);

    let mut timer = Timer::repeating(0.5);
    assert!(!timer.tick(0.25));
    assert!(timer.tick(0.30));
    assert!(timer.finished);
    assert!(timer.elapsed < timer.duration);
    timer.reset();
    assert_eq!(timer.progress(), 0.0);

    let mut world = World::new();
    let empty = spawn_entity(&mut world);
    assert!(world.get::<TemplateTransform>(empty).is_none());

    let positioned = spawn_entity_at(&mut world, Vec3::new(1.0, 2.0, 3.0));
    let transform = world.get::<TemplateTransform>(positioned).unwrap();
    assert_eq!(transform.position, Vec3::new(1.0, 2.0, 3.0));

    assert!(Team::Player.is_hostile_to(&Team::Enemy));
    assert!(!Team::Ally.is_hostile_to(&Team::Player));
}

#[test]
fn fps_weapons_ammo_and_systems_follow_current_api() {
    let mut weapon = Weapon::new(WeaponType::Rifle);
    assert!(weapon.can_fire());
    assert!(weapon.fire());
    assert_eq!(weapon.ammo_in_magazine, weapon.config.magazine_size - 1);
    assert!(!weapon.can_fire());
    assert!(weapon.fire_timer > 0.0);

    weapon.tick(weapon.config.fire_rate);
    weapon.start_reload();
    assert!(weapon.is_reloading);
    assert!(weapon.tick(weapon.config.reload_time));
    assert_eq!(weapon.ammo_in_magazine, weapon.config.magazine_size);

    let mut ammo = Ammo::default();
    ammo.add(WeaponType::Rifle, 60);
    assert_eq!(ammo.consume(WeaponType::Rifle, 17), 17);
    assert_eq!(ammo.get(WeaponType::Rifle), 43);

    let mut world = World::new();
    world.insert_resource(InputState {
        move_axis: Vec3::new(1.0, 0.0, -1.0),
        attack: true,
        reload: false,
        ..Default::default()
    });

    let entity = world.spawn();
    world.insert(
        entity,
        FpsPlayer {
            is_sprinting: true,
            ..Default::default()
        },
    );
    world.insert(entity, TemplateVelocity::default());
    world.insert(entity, Weapon::new(WeaponType::Pistol));

    FpsMovementSystem.run(&mut world);
    let velocity = world.get::<TemplateVelocity>(entity).unwrap();
    assert_eq!(velocity.linear.x, 7.5);
    assert_eq!(velocity.linear.z, 7.5);

    WeaponSystem.run(&mut world);
    let pistol = world.get::<Weapon>(entity).unwrap();
    assert_eq!(
        pistol.ammo_in_magazine,
        WeaponType::Pistol.config().magazine_size - 1
    );

    let enemy = world.spawn();
    world.insert(
        enemy,
        EnemyAI {
            state: EnemyState::Idle,
            patrol_points: vec![Vec3::ZERO, Vec3::ONE],
            ..Default::default()
        },
    );
    EnemyAISystem.run(&mut world);
    assert_eq!(
        world.get::<EnemyAI>(enemy).unwrap().state,
        EnemyState::Patrol
    );

    assert_eq!(FpsPlugin.name(), "fps_template");
}

#[test]
fn rpg_progression_inventory_quests_and_cooldowns_are_consistent() {
    let mut player = RpgPlayer::default();
    assert!(player.add_experience(250));
    assert_eq!(player.level, 3);
    assert_eq!(player.stat_points, 6);
    assert_eq!(player.skill_points, 2);

    let mut stats = CharacterStats::default();
    stats.increase(StatType::Strength, 5);
    stats.increase(StatType::Constitution, 2);
    assert_eq!(stats.physical_damage(), 30.0);
    assert_eq!(stats.max_health(), 170.0);

    let mut inventory = Inventory::new(2);
    let potion = InventoryItem::new(1, "Potion", ItemType::Consumable);
    let sword = InventoryItem::new(2, "Sword", ItemType::Weapon);
    let gold = InventoryItem::new(3, "Gold", ItemType::Gold);
    assert_eq!(inventory.add_item(potion.clone(), 120), potion.max_stack);
    assert_eq!(inventory.add_item(potion, 1), 0);
    assert_eq!(inventory.add_item(sword.clone(), 2), 1);
    assert!(inventory.has_item(sword.id, 1));
    assert_eq!(inventory.add_item(gold, 250), 250);
    assert_eq!(inventory.gold, 250);
    assert!(inventory.is_full());
    assert_eq!(inventory.remove_item(sword.id, 1), 1);

    let mut equipment = Equipment::default();
    let replacement = InventoryItem::new(4, "Axe", ItemType::Weapon);
    assert!(equipment
        .equip(EquipmentSlot::MainHand, sword.clone())
        .is_none());
    let previous = equipment
        .equip(EquipmentSlot::MainHand, replacement)
        .unwrap();
    assert_eq!(previous.id, sword.id);
    assert_eq!(equipment.total_armor(), 10);

    let mut skill = Skill::new(10, "Blink", SkillType::Active);
    assert!(!skill.can_use());
    assert!(skill.upgrade());
    assert!(skill.use_skill());
    assert_eq!(skill.cooldown_timer, skill.cooldown);

    let mut ability = Ability::new(20, "Smite");
    assert!(ability.activate());
    assert!(!ability.is_ready());

    let objective = QuestObjective {
        id: 1,
        description: "Collect three herbs".to_string(),
        objective_type: ObjectiveType::Collect,
        target_id: 7,
        required: 3,
        current: 0,
    };
    let mut quest = Quest::new(99, "Herbal Remedy").add_objective(objective);
    quest.start();
    quest.objectives[0].progress(3);
    quest.complete();
    assert_eq!(quest.state, QuestState::Completed);

    let mut world = World::new();
    let quest_entity = world.spawn();
    let mut active_quest = Quest::new(100, "Already Done").add_objective(QuestObjective {
        id: 2,
        description: "Talk to the elder".to_string(),
        objective_type: ObjectiveType::TalkTo,
        target_id: 1,
        required: 1,
        current: 1,
    });
    active_quest.start();
    world.insert(quest_entity, active_quest);

    let cooldown_entity = world.spawn();
    world.insert(
        cooldown_entity,
        Skill {
            cooldown_timer: 1.0,
            ..Skill::new(11, "Dash", SkillType::Active)
        },
    );
    world.insert(
        cooldown_entity,
        Ability {
            cooldown_timer: 1.0,
            ..Ability::new(21, "Heal")
        },
    );

    RpgQuestSystem.run(&mut world);
    assert_eq!(
        world.get::<Quest>(quest_entity).unwrap().state,
        QuestState::Completed
    );

    RpgSkillSystem.run(&mut world);
    assert!(world.get::<Skill>(cooldown_entity).unwrap().cooldown_timer < 1.0);
    assert!(
        world
            .get::<Ability>(cooldown_entity)
            .unwrap()
            .cooldown_timer
            < 1.0
    );

    assert_eq!(RpgPlugin.name(), "rpg_template");
}

#[test]
fn platformer_helpers_cover_movement_collectibles_and_keys() {
    let mut player = PlatformerPlayer::default();
    assert!(player.can_jump());
    player.jump();
    assert!(player.is_jumping);
    assert_eq!(player.velocity[1], player.jump_force);
    player.apply_gravity(0.5);
    assert!(player.velocity[1] < player.jump_force);
    player.land();
    assert!(player.is_grounded);

    let mut inventory = PlayerInventory::default();
    inventory.add_collectible(&Collectible::coin(5));
    inventory.add_collectible(&Collectible {
        collectible_type: CollectibleType::Key { door_id: 42 },
        value: 1,
        spawn_weight: 1.0,
        bob_speed: 0.0,
        bob_amount: 0.0,
        rotation_speed: 0.0,
        bob_offset: 0.0,
    });
    assert_eq!(inventory.coins, 5);
    assert!(inventory.has_key(42));
    assert!(inventory.use_key(42));
    assert!(!inventory.has_key(42));
}

#[test]
fn rts_resource_selection_and_fog_helpers_are_usable() {
    let cost = ResourceCost::new(50, 25, 10, 0);
    assert_eq!(cost.food + cost.wood + cost.gold + cost.stone, 85);

    let resources = PlayerResources::default();
    assert_eq!(resources.max_population, 5);

    let generator = ResourceGenerator::new(ResourceType::Wood, 1000.0);
    assert_eq!(generator.gather_rate, 0.39);
    assert_eq!(generator.resource_amount, 1000.0);

    let carrying = ResourceCarrying {
        amount: 10.0,
        ..Default::default()
    };
    assert!(carrying.is_full());

    let mut selection = SelectionManager::default();
    selection.select_entity(1);
    selection.add_to_selection(2);
    selection.create_control_group(1);
    selection.clear_selection();
    assert!(selection.select_control_group(1));
    assert_eq!(selection.selected_entities.len(), 2);

    let fog = FogOfWar::new(GlamVec2::new(100.0, 50.0), 10.0);
    assert_eq!(fog.grid_size, [10, 5]);
    assert_eq!(fog.world_to_grid(GlamVec2::new(15.0, 25.0)), Some([1, 2]));

    let input = InputState {
        look_axis: TemplateVec2 { x: 1.0, y: -1.0 },
        ..Default::default()
    };
    assert_eq!(input.look_axis.x, 1.0);
}
