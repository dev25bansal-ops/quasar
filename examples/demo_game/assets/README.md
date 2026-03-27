# Demo Game Assets

This directory should contain game assets for the demo_game example.

## Required Assets

### Models (`assets/models/`)

- `player.glb` - Player character model
- `enemy.glb` - Enemy model
- `pickup.glb` - Collectible item model
- `environment.glb` - Level geometry

### Textures (`assets/textures/`)

- `player_albedo.png` - Player diffuse texture
- `player_normal.png` - Player normal map
- `environment_albedo.png` - Ground/environment texture

### Audio (`assets/audio/`)

- `background_music.ogg` - Background music loop
- `pickup.wav` - Pickup sound effect
- `hit.wav` - Damage/hit sound effect

## Asset Creation

For testing, you can use procedural meshes and colors:

```rust
// In your game code, use MeshShape primitives:
app.world.insert(entity, MeshShape::Cube);  // For player
app.world.insert(entity, MeshShape::Sphere); // For pickups
```

## License

Replace placeholder assets with your own game content.
