import os

os.chdir('D:/quasar')

# 1. ai_editor.rs: Fix import and to_string()
path = 'crates/quasar-editor/src/ai_editor.rs'
with open(path, 'r', encoding='utf-8') as f:
    text = f.read()

text = text.replace('use quasar_core::SimulationState;', 'use quasar_core::SimulationState;\nuse crate::bt_simulation::*;')
text = text.replace('let bb_snapshot: Vec<_> = self.simulation.blackboard_snapshot().into_iter().map(|(k, v): (&String, &quasar_ai::BlackboardValue)| (k.to_string(), v.to_string())).collect();',
                    'let bb_snapshot: Vec<_> = self.simulation.blackboard_snapshot().into_iter().map(|(k, _v)| (k.to_string(), String::from("value"))).collect();')
text = text.replace('let color = match status.as_str() {', 'let status_str: &str = status.as_ref();\n                        let color = match status_str {')

with open(path, 'w', encoding='utf-8') as f:
    f.write(text)

# 2. particle_editor.rs
path = 'crates/quasar-editor/src/particle_editor.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

new_lines = []
skip = 0
for i, line in enumerate(lines):
    if skip > 0:
        skip -= 1
        continue
    # Fix ui.collapsing Shape, Emission, Transform, Color, Physics
    if 'ui.collapsing("Shape", |ui| {' in line and 'self.render_emitter_shape_ui(ui, emitter);' in lines[i+1]:
        new_lines.append('                // Shape\n')
        new_lines.append('                let mut dummy = false;\n')
        new_lines.append('                ui.collapsing("Shape", |ui| { dummy = true; });\n')
        new_lines.append('                if dummy { self.render_emitter_shape_ui(ui, emitter); }\n')
        skip = 2
    elif 'ui.collapsing("Emission", |ui| {' in line and 'self.render_emission_ui(ui, &mut emitter.emission);' in lines[i+1]:
        new_lines.append('                let mut dummy = false;\n')
        new_lines.append('                ui.collapsing("Emission", |ui| { dummy = true; });\n')
        new_lines.append('                if dummy { self.render_emission_ui(ui, &mut emitter.emission); }\n')
        skip = 2
    elif 'ui.collapsing("Transform", |ui| {' in line and 'self.render_transform_ui(ui, &mut emitter.transform);' in lines[i+1]:
        new_lines.append('                let mut dummy = false;\n')
        new_lines.append('                ui.collapsing("Transform", |ui| { dummy = true; });\n')
        new_lines.append('                if dummy { self.render_transform_ui(ui, &mut emitter.transform); }\n')
        skip = 2
    elif 'ui.collapsing("Color", |ui| {' in line and 'ui.label("Start Color:");' in lines[i+1]:
        new_lines.append('                let mut dummy = false;\n')
        new_lines.append('                ui.collapsing("Color", |ui| { dummy = true; });\n')
        new_lines.append('                if dummy {\n')
        new_lines.append('                    ui.label("Start Color:");\n')
        new_lines.append('                    self.render_color_picker(ui, &mut emitter.color_start);\n')
        new_lines.append('                    ui.label("End Color:");\n')
        new_lines.append('                    self.render_color_picker(ui, &mut emitter.color_end);\n')
        new_lines.append('                }\n')
        skip = 6
    # Fix particle modifiers
    elif 'ParticleModifierDef::ColorOverLife(gradient) => {' in line and 'self.render_color_gradient_ui(ui, gradient);' in lines[i+1]:
        new_lines.append('                    ParticleModifierDef::ColorOverLife(ref mut gradient) => {\n')
        new_lines.append('                        let mut g = gradient.clone();\n')
        new_lines.append('                        self.render_color_gradient_ui(ui, &mut g);\n')
        new_lines.append('                        *gradient = g;\n')
        new_lines.append('                    }\n')
        skip = 2
    elif 'ParticleModifierDef::SizeOverLife(curve) => {' in line and 'self.render_curve_ui(ui, curve);' in lines[i+1]:
        new_lines.append('                    ParticleModifierDef::SizeOverLife(ref mut curve) => {\n')
        new_lines.append('                        let mut c = curve.clone();\n')
        new_lines.append('                        self.render_curve_ui(ui, &mut c);\n')
        new_lines.append('                        *curve = c;\n')
        new_lines.append('                    }\n')
        skip = 2
    elif 'ParticleModifierDef::VelocityOverLife(curve) => {' in line and 'self.render_curve_ui(ui, curve);' in lines[i+1]:
        new_lines.append('                    ParticleModifierDef::VelocityOverLife(ref mut curve) => {\n')
        new_lines.append('                        let mut c = curve.clone();\n')
        new_lines.append('                        self.render_curve_ui(ui, &mut c);\n')
        new_lines.append('                        *curve = c;\n')
        new_lines.append('                    }\n')
        skip = 2
    else:
        new_lines.append(line)

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(new_lines)


# 3. quest_editor.rs
path = 'crates/quasar-editor/src/quest_editor.rs'
with open(path, 'r', encoding='utf-8') as f:
    lines = f.readlines()

new_lines = []
skip = 0
for i, line in enumerate(lines):
    if skip > 0:
        skip -= 1
        continue
    
    if 'for (i, objective) in self.quests[idx].objectives.iter_mut().enumerate() {' in line:
        new_lines.append('            for i in 0..self.quests[idx].objectives.len() {\n')
        new_lines.append('                ui.group(|ui| {\n')
        new_lines.append('                    ui.horizontal(|ui| {\n')
        new_lines.append('                        ui.text_edit_singleline(&mut self.quests[idx].objectives[i].description);\n')
        new_lines.append('                        if ui.button("🗑").clicked() {\n')
        new_lines.append('                            delete_objective = Some(i);\n')
        new_lines.append('                        }\n')
        new_lines.append('                    });\n')
        new_lines.append('                });\n')
        new_lines.append('            }\n')
        new_lines.append('            if delete_objective.is_some() { self.save_state(); }\n')
        skip = 10
    elif 'for (i, reward) in self.quests[idx].rewards.iter_mut().enumerate() {' in line:
        new_lines.append('            for i in 0..self.quests[idx].rewards.len() {\n')
        new_lines.append('                ui.group(|ui| {\n')
        new_lines.append('                    ui.horizontal(|ui| {\n')
        new_lines.append('                        match &mut self.quests[idx].rewards[i].reward_type {\n')
        new_lines.append('                            QuestRewardType::Experience(xp) => {\n')
        new_lines.append('                                ui.label("XP:");\n')
        new_lines.append('                                ui.add(egui::DragValue::new(xp));\n')
        new_lines.append('                            }\n')
        new_lines.append('                            QuestRewardType::Item(item_id, count) => {\n')
        new_lines.append('                                ui.label("Item ID:");\n')
        new_lines.append('                                ui.text_edit_singleline(item_id);\n')
        new_lines.append('                                ui.label("Count:");\n')
        new_lines.append('                                ui.add(egui::DragValue::new(count));\n')
        new_lines.append('                            }\n')
        new_lines.append('                            QuestRewardType::Currency(amount) => {\n')
        new_lines.append('                                ui.label("Gold:");\n')
        new_lines.append('                                ui.add(egui::DragValue::new(amount));\n')
        new_lines.append('                            }\n')
        new_lines.append('                            _ => {}\n')
        new_lines.append('                        }\n')
        new_lines.append('                        if ui.button("🗑").clicked() {\n')
        new_lines.append('                            delete_reward = Some(i);\n')
        new_lines.append('                        }\n')
        new_lines.append('                    });\n')
        new_lines.append('                });\n')
        new_lines.append('            }\n')
        new_lines.append('            if delete_reward.is_some() { self.save_state(); }\n')
        skip = 25
    elif 'for (i, prereq) in self.quests[idx].prerequisites.iter_mut().enumerate() {' in line:
        new_lines.append('            for i in 0..self.quests[idx].prerequisites.len() {\n')
        new_lines.append('                ui.group(|ui| {\n')
        new_lines.append('                    ui.horizontal(|ui| {\n')
        new_lines.append('                        match &mut self.quests[idx].prerequisites[i].prereq_type {\n')
        new_lines.append('                            QuestPrerequisiteType::QuestCompleted(quest_id) => {\n')
        new_lines.append('                                ui.label("Quest ID:");\n')
        new_lines.append('                                ui.text_edit_singleline(quest_id);\n')
        new_lines.append('                            }\n')
        new_lines.append('                            QuestPrerequisiteType::Level(level) => {\n')
        new_lines.append('                                ui.label("Level:");\n')
        new_lines.append('                                ui.add(egui::DragValue::new(level));\n')
        new_lines.append('                            }\n')
        new_lines.append('                            _ => {}\n')
        new_lines.append('                        }\n')
        new_lines.append('                        if ui.button("🗑").clicked() {\n')
        new_lines.append('                            delete_prereq = Some(i);\n')
        new_lines.append('                        }\n')
        new_lines.append('                    });\n')
        new_lines.append('                });\n')
        new_lines.append('            }\n')
        new_lines.append('            if delete_prereq.is_some() { self.save_state(); }\n')
        skip = 20
    else:
        new_lines.append(line)

with open(path, 'w', encoding='utf-8') as f:
    f.writelines(new_lines)


# 4. vfx_graph.rs
path = 'crates/quasar-editor/src/vfx_graph.rs'
with open(path, 'r', encoding='utf-8') as f:
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

# 5. splat_editor.rs
path = 'crates/quasar-editor/src/splat_editor.rs'
with open(path, 'r', encoding='utf-8') as f:
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
