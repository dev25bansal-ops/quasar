import re
import os

os.chdir("D:/quasar")

# 1. bt_simulation.rs
path = 'crates/quasar-editor/src/bt_simulation.rs'
with open(path, 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

text = text.replace(
'''        if let Some(ref graph) = self.graph {
            if self.is_running {
                let root_id = graph.root_node?;
                let result = self.execute_node(root_id, graph, 0.016); // 60fps delta

                self.current_status = result;
                
                if let Some(node) = graph.nodes.get(&root_id) {
                    self.add_trace_entry(root_id.0, &node.name, result);
                }''',
'''        let graph_clone = self.graph.clone();
        if let Some(ref graph) = graph_clone {
            if self.is_running {
                let root_id = graph.root_node?;
                let result = self.execute_node(root_id, graph, 0.016); // 60fps delta

                self.current_status = result;
                
                if let Some(node) = graph.nodes.get(&root_id) {
                    self.add_trace_entry(root_id.0, &node.name, result);
                }'''
)

def replace_state_borrow(text):
    text = re.sub(
        r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n\n([ \t]+)// Try children starting from current_child\n([ \t]+)for i in state\.current_child\.\.children\.len\(\) \{\n([ \t]+)state\.current_child = i;\n([ \t]+)let child_status = self\.execute_node\(children\[i\]\.id, graph, dt\);\n\n([ \t]+)match child_status \{',
        r'''let current_child = self.exec_states.get(&node_id.0).unwrap().current_child;

\1// Try children starting from current_child
\2for i in current_child..children.len() {
\3self.exec_states.get_mut(&node_id.0).unwrap().current_child = i;
\4let child_id = children[i].id;
\4let child_status = self.execute_node(child_id, graph, dt);
\4let state = self.exec_states.get_mut(&node_id.0).unwrap();

\5match child_status {''', text)

    text = re.sub(
        r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n\n([ \t]+)for i in state\.current_child\.\.children\.len\(\) \{\n([ \t]+)state\.current_child = i;\n([ \t]+)let child_status = self\.execute_node\(children\[i\]\.id, graph, dt\);\n\n([ \t]+)match child_status \{',
        r'''let current_child = self.exec_states.get(&node_id.0).unwrap().current_child;

\1for i in current_child..children.len() {
\2self.exec_states.get_mut(&node_id.0).unwrap().current_child = i;
\3let child_id = children[i].id;
\3let child_status = self.execute_node(child_id, graph, dt);
\3let state = self.exec_states.get_mut(&node_id.0).unwrap();

\4match child_status {''', text)

    text = re.sub(
        r'state\.current_child = 0;\n([ \t]+)SimNodeStatus::Failure\n([ \t]+)\}',
        r'self.exec_states.get_mut(&node_id.0).unwrap().current_child = 0;\n\1SimNodeStatus::Failure\n\2}', text)

    text = re.sub(
        r'state\.current_child = 0;\n([ \t]+)SimNodeStatus::Success\n([ \t]+)\}',
        r'self.exec_states.get_mut(&node_id.0).unwrap().current_child = 0;\n\1SimNodeStatus::Success\n\2}', text)

    text = re.sub(
        r'if state\.parallel_statuses\.len\(\) != children\.len\(\) \{\n([ \t]+)state\.parallel_statuses = vec!\[SimNodeStatus::Idle; children\.len\(\)\];\n([ \t]+)\}\n\n([ \t]+)let mut success_count = 0;\n([ \t]+)let mut failure_count = 0;\n\n([ \t]+)for \(i, child\) in children\.iter\(\)\.enumerate\(\) \{\n([ \t]+)if state\.parallel_statuses\[i\] != SimNodeStatus::Success\n([ \t]+)&& state\.parallel_statuses\[i\] != SimNodeStatus::Failure\n([ \t]+)\{\n([ \t]+)state\.parallel_statuses\[i\] = self\.execute_node\(child\.id, graph, dt\);\n([ \t]+)\}\n\n([ \t]+)match state\.parallel_statuses\[i\] \{',
        r'''if self.exec_states.get(&node_id.0).unwrap().parallel_statuses.len() != children.len() {
\1self.exec_states.get_mut(&node_id.0).unwrap().parallel_statuses = vec![SimNodeStatus::Idle; children.len()];
\2}

\3let mut success_count = 0;
\4let mut failure_count = 0;

\5for (i, child) in children.iter().enumerate() {
\6if self.exec_states.get(&node_id.0).unwrap().parallel_statuses[i] != SimNodeStatus::Success
\7&& self.exec_states.get(&node_id.0).unwrap().parallel_statuses[i] != SimNodeStatus::Failure
\8{
\9let child_id = child.id;
\9let child_status = self.execute_node(child_id, graph, dt);
\9self.exec_states.get_mut(&node_id.0).unwrap().parallel_statuses[i] = child_status;
\10}

\11match self.exec_states.get(&node_id.0).unwrap().parallel_statuses[i] {''', text)

    text = re.sub(
        r'state\.parallel_statuses\.clear\(\);\n([ \t]+)state\.current_child = 0;\n([ \t]+)return SimNodeStatus::Success;',
        r'let state = self.exec_states.get_mut(&node_id.0).unwrap();\n\1state.parallel_statuses.clear();\n\1state.current_child = 0;\n\2return SimNodeStatus::Success;', text)

    text = re.sub(
        r'state\.parallel_statuses\.clear\(\);\n([ \t]+)state\.current_child = 0;\n([ \t]+)return SimNodeStatus::Failure;',
        r'let state = self.exec_states.get_mut(&node_id.0).unwrap();\n\1state.parallel_statuses.clear();\n\1state.current_child = 0;\n\2return SimNodeStatus::Failure;', text)

    text = re.sub(
        r'let start_idx = state\.current_child;\n([ \t]+)for offset in 0\.\.children\.len\(\) \{\n([ \t]+)let idx = \(start_idx \+ offset\) % children\.len\(\);\n([ \t]+)let child_status = self\.execute_node\(children\[idx\]\.id, graph, dt\);\n\n([ \t]+)match child_status \{',
        r'''let start_idx = self.exec_states.get(&node_id.0).unwrap().current_child;
\1for offset in 0..children.len() {
\2let idx = (start_idx + offset) % children.len();
\3let child_id = children[idx].id;
\3let child_status = self.execute_node(child_id, graph, dt);
\3let state = self.exec_states.get_mut(&node_id.0).unwrap();

\4match child_status {''', text)

    text = re.sub(
        r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n([ \t]+)let max_count = graph\.nodes\.get\(&node_id\)\n([ \t]+)\.and_then\(\|n\| n\.properties\.get\("count"\)\)\n([ \t]+)\.and_then\(\|s\| s\.parse::<i32>\(\)\.ok\(\)\)\n([ \t]+)\.unwrap_or\(-1\); // -1 = infinite\n\n([ \t]+)if max_count >= 0 && state\.retry_count >= max_count as u32 \{\n([ \t]+)state\.retry_count = 0;\n([ \t]+)return SimNodeStatus::Success;\n([ \t]+)\}\n\n([ \t]+)let child_status = self\.execute_node\(children\[0\]\.id, graph, dt\);\n([ \t]+)match child_status \{',
        r'''let max_count = graph.nodes.get(&node_id)
\2.and_then(|n| n.properties.get("count"))
\3.and_then(|s| s.parse::<i32>().ok())
\4.unwrap_or(-1); // -1 = infinite

\5if max_count >= 0 && self.exec_states.get(&node_id.0).unwrap().retry_count >= max_count as u32 {
\6self.exec_states.get_mut(&node_id.0).unwrap().retry_count = 0;
\7return SimNodeStatus::Success;
\8}

\9let child_id = children[0].id;
\9let child_status = self.execute_node(child_id, graph, dt);
\9let state = self.exec_states.get_mut(&node_id.0).unwrap();
\10match child_status {''', text)

    text = re.sub(
        r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n([ \t]+)let max_count = graph\.nodes\.get\(&node_id\)\n([ \t]+)\.and_then\(\|n\| n\.properties\.get\("max_retries"\)\)\n([ \t]+)\.and_then\(\|s\| s\.parse::<i32>\(\)\.ok\(\)\)\n([ \t]+)\.unwrap_or\(-1\); // -1 = infinite\n\n([ \t]+)if max_count >= 0 && state\.retry_count >= max_count as u32 \{\n([ \t]+)state\.retry_count = 0;\n([ \t]+)return SimNodeStatus::Failure;\n([ \t]+)\}\n\n([ \t]+)let child_status = self\.execute_node\(children\[0\]\.id, graph, dt\);\n([ \t]+)match child_status \{',
        r'''let max_count = graph.nodes.get(&node_id)
\2.and_then(|n| n.properties.get("max_retries"))
\3.and_then(|s| s.parse::<i32>().ok())
\4.unwrap_or(-1); // -1 = infinite

\5if max_count >= 0 && self.exec_states.get(&node_id.0).unwrap().retry_count >= max_count as u32 {
\6self.exec_states.get_mut(&node_id.0).unwrap().retry_count = 0;
\7return SimNodeStatus::Failure;
\8}

\9let child_id = children[0].id;
\9let child_status = self.execute_node(child_id, graph, dt);
\9let state = self.exec_states.get_mut(&node_id.0).unwrap();
\10match child_status {''', text)

    text = re.sub(
        r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n([ \t]+)let duration = graph\.nodes\.get\(&node_id\)\n([ \t]+)\.and_then\(\|n\| n\.properties\.get\("duration"\)\)\n([ \t]+)\.and_then\(\|s\| s\.parse::<f32>\(\)\.ok\(\)\)\n([ \t]+)\.unwrap_or\(1\.0\);\n\n([ \t]+)let child_status = self\.execute_node\(children\[0\]\.id, graph, dt\);\n([ \t]+)if child_status != SimNodeStatus::Running \{\n([ \t]+)state\.elapsed_time = 0\.0;\n([ \t]+)return child_status;\n([ \t]+)\}',
        r'''let duration = graph.nodes.get(&node_id)
\2.and_then(|n| n.properties.get("duration"))
\3.and_then(|s| s.parse::<f32>().ok())
\4.unwrap_or(1.0);

\5let child_id = children[0].id;
\5let child_status = self.execute_node(child_id, graph, dt);
\6if child_status != SimNodeStatus::Running {
\7self.exec_states.get_mut(&node_id.0).unwrap().elapsed_time = 0.0;
\8return child_status;
\9}
\9let state = self.exec_states.get_mut(&node_id.0).unwrap();''', text)

    text = re.sub(
        r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n([ \t]+)let duration = graph\.nodes\.get\(&node_id\)\n([ \t]+)\.and_then\(\|n\| n\.properties\.get\("duration"\)\)\n([ \t]+)\.and_then\(\|s\| s\.parse::<f32>\(\)\.ok\(\)\)\n([ \t]+)\.unwrap_or\(1\.0\);\n\n([ \t]+)// If we are cooling down\n([ \t]+)if state\.elapsed_time > 0\.0 && state\.elapsed_time < duration \{\n([ \t]+)state\.elapsed_time \+= dt;\n([ \t]+)return SimNodeStatus::Failure;\n([ \t]+)\}\n\n([ \t]+)// Otherwise execute child\n([ \t]+)let child_status = self\.execute_node\(children\[0\]\.id, graph, dt\);\n([ \t]+)if child_status == SimNodeStatus::Success || child_status == SimNodeStatus::Failure \{\n([ \t]+)state\.elapsed_time = 0\.0001; // Start cooldown\n([ \t]+)\}\n([ \t]+)child_status',
        r'''let duration = graph.nodes.get(&node_id)
\2.and_then(|n| n.properties.get("duration"))
\3.and_then(|s| s.parse::<f32>().ok())
\4.unwrap_or(1.0);

\5// If we are cooling down
\6if self.exec_states.get(&node_id.0).unwrap().elapsed_time > 0.0 && self.exec_states.get(&node_id.0).unwrap().elapsed_time < duration {
\7self.exec_states.get_mut(&node_id.0).unwrap().elapsed_time += dt;
\8return SimNodeStatus::Failure;
\9}

\10// Otherwise execute child
\11let child_id = children[0].id;
\11let child_status = self.execute_node(child_id, graph, dt);
\12if child_status == SimNodeStatus::Success || child_status == SimNodeStatus::Failure {
\13self.exec_states.get_mut(&node_id.0).unwrap().elapsed_time = 0.0001; // Start cooldown
\14}
\15child_status''', text)
    return text

text = replace_state_borrow(text)
text = re.sub(r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n\n([ \t]+)if self\.exec_states', r'\n\1if self.exec_states', text)
text = re.sub(r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n\n([ \t]+)let max_count', r'\n\1let max_count', text)
text = re.sub(r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n([ \t]+)let duration', r'\n\1let duration', text)
text = re.sub(r'let state = self\.exec_states\.get_mut\(&node_id\.0\)\.unwrap\(\);\n([ \t]+)// Shuffle children order', r'\n\1let state = self.exec_states.get_mut(&node_id.0).unwrap();\n\1// Shuffle children order', text)

with open(path, 'w', encoding='utf-8') as f:
    f.write(text)

# 2. foliage_editor.rs
path = 'crates/quasar-editor/src/foliage_editor.rs'
with open(path, 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

text = text.replace(
'''                ui.horizontal(|ui| {
                    if ui.selectable_label(is_selected, &ftype.name).clicked() {
                        state.selected_type = i;
                        state.update_brush_for_selection();
                    }
                    if ui.button("🗑").clicked() {
                        // Handle deletion later
                    }
                });''',
'''                let mut should_update = false;
                ui.horizontal(|ui| {
                    if ui.selectable_label(is_selected, &ftype.name).clicked() {
                        should_update = true;
                    }
                    if ui.button("🗑").clicked() {
                        // Handle deletion later
                    }
                });
                if should_update {
                    state.selected_type = i;
                    state.update_brush_for_selection();
                }'''
)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)

# 3. particle_editor.rs
path = 'crates/quasar-editor/src/particle_editor.rs'
with open(path, 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

text = text.replace(
'''                if let Some(path) = &self.file_path {
                    if let Err(e) = self.save_to_file(path) {''',
'''                let path_clone = self.file_path.clone();
                if let Some(path) = path_clone {
                    if let Err(e) = self.save_to_file(&path) {'''
)

text = text.replace(
'''            if let Some(emitter) = self.system_def.emitters.get_mut(idx) {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut emitter.name);
                });

                ui.collapsing("Shape", |ui| {
                    self.render_emitter_shape_ui(ui, emitter);
                });

                ui.collapsing("Emission", |ui| {
                    self.render_emission_ui(ui, &mut emitter.emission);
                });

                ui.collapsing("Transform", |ui| {
                    self.render_transform_ui(ui, &mut emitter.transform);
                });

                ui.collapsing("Color", |ui| {
                    ui.label("Start Color:");
                    self.render_color_picker(ui, &mut emitter.color_start);
                    ui.label("End Color:");
                    self.render_color_picker(ui, &mut emitter.color_end);
                });

                ui.collapsing("Physics", |ui| {
                    ui.add(egui::Checkbox::new(&mut emitter.physics.collide_with_world, "Collide with World"));
                    ui.horizontal(|ui| {
                        ui.label("Bounciness:");
                        ui.add(egui::DragValue::new(&mut emitter.physics.bounciness).speed(0.1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Friction:");
                        ui.add(egui::DragValue::new(&mut emitter.physics.friction).speed(0.1));
                    });
                });
            }''',
'''            let mut should_render_shape = false;
            let mut should_render_emission = false;
            let mut should_render_transform = false;
            let mut should_render_color = false;
            let mut should_render_physics = false;

            if let Some(emitter) = self.system_def.emitters.get_mut(idx) {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut emitter.name);
                });

                ui.collapsing("Shape", |ui| { should_render_shape = true; });
                ui.collapsing("Emission", |ui| { should_render_emission = true; });
                ui.collapsing("Transform", |ui| { should_render_transform = true; });
                ui.collapsing("Color", |ui| { should_render_color = true; });
                ui.collapsing("Physics", |ui| { should_render_physics = true; });
            }

            if should_render_shape {
                if let Some(emitter) = self.system_def.emitters.get_mut(idx) {
                    self.render_emitter_shape_ui(ui, emitter);
                }
            }
            if should_render_emission {
                if let Some(emitter) = self.system_def.emitters.get_mut(idx) {
                    self.render_emission_ui(ui, &mut emitter.emission);
                }
            }
            if should_render_transform {
                if let Some(emitter) = self.system_def.emitters.get_mut(idx) {
                    self.render_transform_ui(ui, &mut emitter.transform);
                }
            }
            if should_render_color {
                if let Some(emitter) = self.system_def.emitters.get_mut(idx) {
                    ui.label("Start Color:");
                    self.render_color_picker(ui, &mut emitter.color_start);
                    ui.label("End Color:");
                    self.render_color_picker(ui, &mut emitter.color_end);
                }
            }
            if should_render_physics {
                if let Some(emitter) = self.system_def.emitters.get_mut(idx) {
                    ui.add(egui::Checkbox::new(&mut emitter.physics.collide_with_world, "Collide with World"));
                    ui.horizontal(|ui| {
                        ui.label("Bounciness:");
                        ui.add(egui::DragValue::new(&mut emitter.physics.bounciness).speed(0.1));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Friction:");
                        ui.add(egui::DragValue::new(&mut emitter.physics.friction).speed(0.1));
                    });
                }
            }'''
)

text = text.replace(
'''            if let Some(modifier) = self.system_def.modifiers.get_mut(idx) {
                match modifier {
                    ParticleModifierDef::ColorOverLife(gradient) => {
                        self.render_color_gradient_ui(ui, gradient);
                    }
                    ParticleModifierDef::SizeOverLife(curve) => {
                        self.render_curve_ui(ui, curve);
                    }
                    ParticleModifierDef::VelocityOverLife(curve) => {
                        self.render_curve_ui(ui, curve);
                    }
                    _ => {}
                }
            }''',
'''            let mut gradient_to_render = None;
            let mut curve_to_render = None;
            if let Some(modifier) = self.system_def.modifiers.get_mut(idx) {
                match modifier {
                    ParticleModifierDef::ColorOverLife(gradient) => {
                        gradient_to_render = Some(gradient.clone());
                    }
                    ParticleModifierDef::SizeOverLife(curve) | ParticleModifierDef::VelocityOverLife(curve) => {
                        curve_to_render = Some(curve.clone());
                    }
                    _ => {}
                }
            }
            if let Some(mut gradient) = gradient_to_render {
                self.render_color_gradient_ui(ui, &mut gradient);
                if let Some(ParticleModifierDef::ColorOverLife(g)) = self.system_def.modifiers.get_mut(idx) {
                    *g = gradient;
                }
            }
            if let Some(mut curve) = curve_to_render {
                self.render_curve_ui(ui, &mut curve);
                if let Some(modifier) = self.system_def.modifiers.get_mut(idx) {
                    match modifier {
                        ParticleModifierDef::SizeOverLife(c) | ParticleModifierDef::VelocityOverLife(c) => {
                            *c = curve;
                        }
                        _ => {}
                    }
                }
            }'''
)

text = text.replace(
'''        self.render_color_picker(ui, &mut self.background_color);''',
'''        let mut bg_color = self.background_color;
        self.render_color_picker(ui, &mut bg_color);
        self.background_color = bg_color;'''
)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)

# 4. quest_editor.rs
path = 'crates/quasar-editor/src/quest_editor.rs'
with open(path, 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

text = text.replace(
'''            for (i, objective) in self.quests[idx].objectives.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut objective.description);
                        if ui.button("🗑").clicked() {
                            delete_objective = Some(i);
                            self.save_state();
                        }
                    });
                });
            }''',
'''            for i in 0..self.quests[idx].objectives.len() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.text_edit_singleline(&mut self.quests[idx].objectives[i].description);
                        if ui.button("🗑").clicked() {
                            delete_objective = Some(i);
                        }
                    });
                });
            }
            if delete_objective.is_some() { self.save_state(); }'''
)

text = text.replace(
'''            for (i, reward) in self.quests[idx].rewards.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        match &mut reward.reward_type {
                            QuestRewardType::Experience(xp) => {
                                ui.label("XP:");
                                ui.add(egui::DragValue::new(xp));
                            }
                            QuestRewardType::Item(item_id, count) => {
                                ui.label("Item ID:");
                                ui.text_edit_singleline(item_id);
                                ui.label("Count:");
                                ui.add(egui::DragValue::new(count));
                            }
                            QuestRewardType::Currency(amount) => {
                                ui.label("Gold:");
                                ui.add(egui::DragValue::new(amount));
                            }
                            _ => {}
                        }
                        if ui.button("🗑").clicked() {
                            delete_reward = Some(i);
                            self.save_state();
                        }
                    });
                });
            }''',
'''            for i in 0..self.quests[idx].rewards.len() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        match &mut self.quests[idx].rewards[i].reward_type {
                            QuestRewardType::Experience(xp) => {
                                ui.label("XP:");
                                ui.add(egui::DragValue::new(xp));
                            }
                            QuestRewardType::Item(item_id, count) => {
                                ui.label("Item ID:");
                                ui.text_edit_singleline(item_id);
                                ui.label("Count:");
                                ui.add(egui::DragValue::new(count));
                            }
                            QuestRewardType::Currency(amount) => {
                                ui.label("Gold:");
                                ui.add(egui::DragValue::new(amount));
                            }
                            _ => {}
                        }
                        if ui.button("🗑").clicked() {
                            delete_reward = Some(i);
                        }
                    });
                });
            }
            if delete_reward.is_some() { self.save_state(); }'''
)

text = text.replace(
'''            for (i, prereq) in self.quests[idx].prerequisites.iter_mut().enumerate() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        match &mut prereq.prereq_type {
                            QuestPrerequisiteType::QuestCompleted(quest_id) => {
                                ui.label("Quest ID:");
                                ui.text_edit_singleline(quest_id);
                            }
                            QuestPrerequisiteType::Level(level) => {
                                ui.label("Level:");
                                ui.add(egui::DragValue::new(level));
                            }
                            _ => {}
                        }
                        if ui.button("🗑").clicked() {
                            delete_prereq = Some(i);
                            self.save_state();
                        }
                    });
                });
            }''',
'''            for i in 0..self.quests[idx].prerequisites.len() {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        match &mut self.quests[idx].prerequisites[i].prereq_type {
                            QuestPrerequisiteType::QuestCompleted(quest_id) => {
                                ui.label("Quest ID:");
                                ui.text_edit_singleline(quest_id);
                            }
                            QuestPrerequisiteType::Level(level) => {
                                ui.label("Level:");
                                ui.add(egui::DragValue::new(level));
                            }
                            _ => {}
                        }
                        if ui.button("🗑").clicked() {
                            delete_prereq = Some(i);
                        }
                    });
                });
            }
            if delete_prereq.is_some() { self.save_state(); }'''
)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)

# 5. splat_editor.rs
path = 'crates/quasar-editor/src/splat_editor.rs'
with open(path, 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

text = text.replace(
'''                ui.horizontal(|ui| {
                    if ui.selectable_label(is_selected, format!("Material {}", i)).clicked() {
                        state.selected_channel = i;
                        state.update_brush_channel();
                    }
                    if ui.button("🗑").clicked() {
                        // Handle deletion later
                    }
                });''',
'''                let mut should_update = false;
                ui.horizontal(|ui| {
                    if ui.selectable_label(is_selected, format!("Material {}", i)).clicked() {
                        should_update = true;
                    }
                    if ui.button("🗑").clicked() {
                        // Handle deletion later
                    }
                });
                if should_update {
                    state.selected_channel = i;
                    state.update_brush_channel();
                }'''
)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)

# 6. vfx_graph.rs
path = 'crates/quasar-editor/src/vfx_graph.rs'
with open(path, 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

text = text.replace(
'''        self.graph.nodes.iter_mut().rev().find(|node| {
            let node_pos = Pos2::new(node.position.x, node.position.y);
            let rect = Rect::from_min_size(node_pos, Vec2::new(size.x, self.estimate_node_height(node)));
            rect.contains(pos)
        })''',
'''        let mut found_id = None;
        for node in self.graph.nodes.iter().rev() {
            let node_pos = Pos2::new(node.position.x, node.position.y);
            let rect = Rect::from_min_size(node_pos, Vec2::new(size.x, self.estimate_node_height(node)));
            if rect.contains(pos) {
                found_id = Some(node.id);
                break;
            }
        }
        if let Some(id) = found_id {
            self.graph.nodes.iter_mut().find(|n| n.id == id)
        } else {
            None
        }'''
)
with open(path, 'w', encoding='utf-8') as f:
    f.write(text)
