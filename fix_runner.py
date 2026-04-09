#!/usr/bin/env python3
import re

with open("D:/quasar/crates/quasar-engine/src/runner.rs", "r", encoding="utf-8") as f:
    content = f.read()

# 1. Add cfg attribute to quasar_editor import
content = content.replace(
    "use quasar_editor::{renderer::EditorRenderer, Editor};",
    '#[cfg(feature = "editor")]\nuse quasar_editor::{renderer::EditorRenderer, Editor};',
)

# 2. Add cfg attribute to quasar_ui import
content = content.replace(
    "use quasar_ui::{UiRenderPass, UiResource};",
    '#[cfg(feature = "ui")]\nuse quasar_ui::{UiRenderPass, UiResource};',
)

# 3. Add cfg attributes to editor and editor_renderer fields in RunnerState
content = content.replace(
    "    orbit: OrbitController,\n    editor: Editor,\n    editor_renderer: EditorRenderer,\n    mesh_cache: MeshCache,",
    '    orbit: OrbitController,\n    #[cfg(feature = "editor")]\n    editor: Editor,\n    #[cfg(feature = "editor")]\n    editor_renderer: EditorRenderer,\n    mesh_cache: MeshCache,',
)

# 4. Add cfg attribute to ui_render_pass field in RunnerState
content = content.replace(
    "    /// UI render pass for in-game UI\n    ui_render_pass: UiRenderPass,\n}",
    '    /// UI render pass for in-game UI\n    #[cfg(feature = "ui")]\n    ui_render_pass: UiRenderPass,\n}',
)

# 5. Add cfg attribute before UiPlugin registration
content = content.replace(
    "        // Register UiPlugin for in-game UI\n        self.app.add_plugin(quasar_ui::UiPlugin);",
    '        // Register UiPlugin for in-game UI\n        #[cfg(feature = "ui")]\n        self.app.add_plugin(quasar_ui::UiPlugin);',
)

# 6. Add cfg attribute before ui_render_pass creation
content = content.replace(
    "        // Create UI render pass for in-game UI\n        let ui_render_pass = UiRenderPass::new(&renderer.device, renderer.config.format);",
    '        // Create UI render pass for in-game UI\n        #[cfg(feature = "ui")]\n        let ui_render_pass = UiRenderPass::new(&renderer.device, renderer.config.format);',
)

# 7. Add cfg attributes before editor and editor_renderer in RunnerState initialization
content = content.replace(
    "            orbit: OrbitController::new(5.0),\n            editor,\n            editor_renderer,\n            mesh_cache: MeshCache::new(),",
    '            orbit: OrbitController::new(5.0),\n            #[cfg(feature = "editor")]\n            editor,\n            #[cfg(feature = "editor")]\n            editor_renderer,\n            mesh_cache: MeshCache::new(),',
)

# 8. Add cfg attribute before ui_render_pass in RunnerState initialization
content = content.replace(
    "            deferred_lighting,\n            ui_render_pass,\n        });",
    '            deferred_lighting,\n            #[cfg(feature = "ui")]\n            ui_render_pass,\n        });',
)

# 9. Wrap egui_consumed in cfg blocks
content = content.replace(
    "        // ── Let egui have first crack at the event ────────────────\n        let egui_consumed = state.editor_renderer.handle_event(&state.window, &event);",
    '        // ── Let egui have first crack at the event ────────────────\n        #[cfg(feature = "editor")]\n        let egui_consumed = state.editor_renderer.handle_event(&state.window, &event);\n        #[cfg(not(feature = "editor"))]\n        let egui_consumed = false;',
)

# 10. Wrap editor.toggle() in #[cfg(feature = "editor")]
content = content.replace(
    "                    // F12 toggles the editor regardless.\n                    if key == KeyCode::F12 && event.state.is_pressed() {\n                        state.editor.toggle();\n                    }",
    '                    // F12 toggles the editor regardless.\n                    #[cfg(feature = "editor")]\n                    if key == KeyCode::F12 && event.state.is_pressed() {\n                        state.editor.toggle();\n                    }',
)

# 11. Wrap SimulationState should_tick with cfg blocks
content = content.replace(
    "                self.app\n                    .world\n                    .insert_resource(quasar_core::SimulationState {\n                        should_tick: state.editor.state.should_tick(),\n                    });",
    '                self.app.world.insert_resource(quasar_core::SimulationState {\n                    #[cfg(feature = "editor")]\n                    should_tick: state.editor.state.should_tick(),\n                    #[cfg(not(feature = "editor"))]\n                    should_tick: true,\n                });',
)

# 12. Wrap editor-related code in #[cfg(feature = "editor")]
content = content.replace(
    "                // Periodically check for asset changes (every ~1 second when playing)\n                if state.editor.state.should_tick() {\n                    let frame_count = self\n                        .app\n                        .world\n                        .resource::<TimeSnapshot>()\n                        .map(|t| t.frame_count)\n                        .unwrap_or(0);\n                    if frame_count.is_multiple_of(60) {\n                        state.editor.check_asset_changes();\n                    }\n                }",
    '                // Periodically check for asset changes (every ~1 second when playing)\n                #[cfg(feature = "editor")]\n                if state.editor.state.should_tick() {\n                    let frame_count = self\n                        .app\n                        .world\n                        .resource::<TimeSnapshot>()\n                        .map(|t| t.frame_count)\n                        .unwrap_or(0);\n                    if frame_count.is_multiple_of(60) {\n                        state.editor.check_asset_changes();\n                    }\n                }',
)

# 13. Wrap UI pass in #[cfg(feature = "ui")]
content = content.replace(
    "                        // In-game UI pass (rendered after 3D/tonemapping, before editor)\n                        if let Some(ui_resource) = self.app.world.resource::<UiResource>() {",
    '                        // In-game UI pass (rendered after 3D/tonemapping, before editor)\n                        #[cfg(feature = "ui")]\n                        if let Some(ui_resource) = self.app.world.resource::<UiResource>() {',
)

# 14. Wrap editor egui pass in cfg blocks
content = content.replace(
    "                        // egui pass (editor overlay).\n                        let egui_commands = if state.editor.enabled {",
    '                        // egui pass (editor overlay).\n                        #[cfg(feature = "editor")]\n                        let egui_commands = if state.editor.enabled {',
)

# 15. Add fallback for egui_commands when editor is not enabled
content = content.replace(
    "                        } else {\n                            None\n                        };\n\n                        // Resolve GPU profiler timestamps before submit.",
    '                        } else {\n                            None\n                        };\n                        #[cfg(not(feature = "editor"))]\n                        let egui_commands: Option<wgpu::CommandBuffer> = None;\n\n                        // Resolve GPU profiler timestamps before submit.',
)

# 16. Wrap GPU profiler collection for editor in #[cfg(feature = "editor")]
content = content.replace(
    "                        // Collect GPU profiler results and feed to editor.\n                        state.gpu_profiler.request_results();\n                        if let Some(timings) =\n                            state.gpu_profiler.try_collect(&state.renderer.device)\n                        {\n                            state.editor.gpu_pass_timings = timings.to_vec();\n                        }",
    '                        // Collect GPU profiler results and feed to editor.\n                        #[cfg(feature = "editor")]\n                        {\n                            state.gpu_profiler.request_results();\n                            if let Some(timings) =\n                                state.gpu_profiler.try_collect(&state.renderer.device)\n                            {\n                                state.editor.gpu_pass_timings = timings.to_vec();\n                            }\n                        }',
)

with open("D:/quasar/crates/quasar-engine/src/runner.rs", "w", encoding="utf-8") as f:
    f.write(content)

print("File updated successfully")
