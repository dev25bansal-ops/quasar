import os

os.chdir("D:/quasar")

path = 'crates/quasar-editor/src/bt_simulation.rs'
with open(path, 'r', encoding='utf-8', errors='ignore') as f:
    text = f.read()

# Instead of regex, let's just do targeted replacements!

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

text = text.replace(
'''    fn tick_selector(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        let state = self.exec_states.get_mut(&node_id.0).unwrap();

        // Try children starting from current_child
        for i in state.current_child..children.len() {
            state.current_child = i;
            let child_status = self.execute_node(children[i].id, graph, dt);

            match child_status {
                SimNodeStatus::Success => {
                    state.current_child = 0;
                    return SimNodeStatus::Success;
                }
                SimNodeStatus::Running => {
                    return SimNodeStatus::Running;
                }
                SimNodeStatus::Failure => {
                    continue;
                }
                _ => {}
            }
        }

        state.current_child = 0;
        SimNodeStatus::Failure
    }''',
'''    fn tick_selector(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        let mut current_child = self.exec_states.get(&node_id.0).unwrap().current_child;

        // Try children starting from current_child
        for i in current_child..children.len() {
            self.exec_states.get_mut(&node_id.0).unwrap().current_child = i;
            let child_id = children[i].id;
            let child_status = self.execute_node(child_id, graph, dt);
            
            let state = self.exec_states.get_mut(&node_id.0).unwrap();
            match child_status {
                SimNodeStatus::Success => {
                    state.current_child = 0;
                    return SimNodeStatus::Success;
                }
                SimNodeStatus::Running => {
                    return SimNodeStatus::Running;
                }
                SimNodeStatus::Failure => {
                    continue;
                }
                _ => {}
            }
        }

        self.exec_states.get_mut(&node_id.0).unwrap().current_child = 0;
        SimNodeStatus::Failure
    }'''
)

text = text.replace(
'''    fn tick_sequence(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        let state = self.exec_states.get_mut(&node_id.0).unwrap();

        for i in state.current_child..children.len() {
            state.current_child = i;
            let child_status = self.execute_node(children[i].id, graph, dt);

            match child_status {
                SimNodeStatus::Success => {
                    continue;
                }
                SimNodeStatus::Running => {
                    return SimNodeStatus::Running;
                }
                SimNodeStatus::Failure => {
                    state.current_child = 0;
                    return SimNodeStatus::Failure;
                }
                _ => {}
            }
        }

        state.current_child = 0;
        SimNodeStatus::Success
    }''',
'''    fn tick_sequence(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        let current_child = self.exec_states.get(&node_id.0).unwrap().current_child;

        for i in current_child..children.len() {
            self.exec_states.get_mut(&node_id.0).unwrap().current_child = i;
            let child_id = children[i].id;
            let child_status = self.execute_node(child_id, graph, dt);
            
            let state = self.exec_states.get_mut(&node_id.0).unwrap();
            match child_status {
                SimNodeStatus::Success => {
                    continue;
                }
                SimNodeStatus::Running => {
                    return SimNodeStatus::Running;
                }
                SimNodeStatus::Failure => {
                    state.current_child = 0;
                    return SimNodeStatus::Failure;
                }
                _ => {}
            }
        }

        self.exec_states.get_mut(&node_id.0).unwrap().current_child = 0;
        SimNodeStatus::Success
    }'''
)

text = text.replace(
'''    fn tick_parallel(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        let state = self.exec_states.get_mut(&node_id.0).unwrap();

        if state.parallel_statuses.len() != children.len() {
            state.parallel_statuses = vec![SimNodeStatus::Idle; children.len()];
        }

        let mut success_count = 0;
        let mut failure_count = 0;

        for (i, child) in children.iter().enumerate() {
            if state.parallel_statuses[i] != SimNodeStatus::Success
                && state.parallel_statuses[i] != SimNodeStatus::Failure
            {
                state.parallel_statuses[i] = self.execute_node(child.id, graph, dt);
            }

            match state.parallel_statuses[i] {
                SimNodeStatus::Success => success_count += 1,
                SimNodeStatus::Failure => failure_count += 1,
                SimNodeStatus::Running => {}
                _ => {}
            }
        }

        // Check policy
        let policy = graph.nodes.get(&node_id)
            .and_then(|n| n.properties.get("policy"))
            .cloned()
            .unwrap_or_else(|| "RequireAll".to_string());

        if policy == "RequireOne" && success_count > 0 {
            state.parallel_statuses.clear();
            state.current_child = 0;
            return SimNodeStatus::Success;
        }

        if success_count == children.len() {
            state.parallel_statuses.clear();
            state.current_child = 0;
            return SimNodeStatus::Success;
        }

        if failure_count > 0 && policy == "RequireAll" {
            state.parallel_statuses.clear();
            state.current_child = 0;
            return SimNodeStatus::Failure;
        }

        if failure_count == children.len() {
            state.parallel_statuses.clear();
            state.current_child = 0;
            return SimNodeStatus::Failure;
        }

        SimNodeStatus::Running
    }''',
'''    fn tick_parallel(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        
        if self.exec_states.get(&node_id.0).unwrap().parallel_statuses.len() != children.len() {
            self.exec_states.get_mut(&node_id.0).unwrap().parallel_statuses = vec![SimNodeStatus::Idle; children.len()];
        }

        let mut success_count = 0;
        let mut failure_count = 0;

        for (i, child) in children.iter().enumerate() {
            if self.exec_states.get(&node_id.0).unwrap().parallel_statuses[i] != SimNodeStatus::Success
                && self.exec_states.get(&node_id.0).unwrap().parallel_statuses[i] != SimNodeStatus::Failure
            {
                let child_id = child.id;
                let child_status = self.execute_node(child_id, graph, dt);
                self.exec_states.get_mut(&node_id.0).unwrap().parallel_statuses[i] = child_status;
            }

            match self.exec_states.get(&node_id.0).unwrap().parallel_statuses[i] {
                SimNodeStatus::Success => success_count += 1,
                SimNodeStatus::Failure => failure_count += 1,
                SimNodeStatus::Running => {}
                _ => {}
            }
        }

        // Check policy
        let policy = graph.nodes.get(&node_id)
            .and_then(|n| n.properties.get("policy"))
            .cloned()
            .unwrap_or_else(|| "RequireAll".to_string());

        if policy == "RequireOne" && success_count > 0 {
            let state = self.exec_states.get_mut(&node_id.0).unwrap();
            state.parallel_statuses.clear();
            state.current_child = 0;
            return SimNodeStatus::Success;
        }

        if success_count == children.len() {
            let state = self.exec_states.get_mut(&node_id.0).unwrap();
            state.parallel_statuses.clear();
            state.current_child = 0;
            return SimNodeStatus::Success;
        }

        if failure_count > 0 && policy == "RequireAll" {
            let state = self.exec_states.get_mut(&node_id.0).unwrap();
            state.parallel_statuses.clear();
            state.current_child = 0;
            return SimNodeStatus::Failure;
        }

        if failure_count == children.len() {
            let state = self.exec_states.get_mut(&node_id.0).unwrap();
            state.parallel_statuses.clear();
            state.current_child = 0;
            return SimNodeStatus::Failure;
        }

        SimNodeStatus::Running
    }'''
)

text = text.replace(
'''    fn tick_random_selector(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        let state = self.exec_states.get_mut(&node_id.0).unwrap();

        // Shuffle children order (simulated with state)
        if state.current_child == 0 {
            // Use a simple pseudo-random based on tick count
            state.current_child = (self.tick_count as usize) % children.len().max(1);
        }

        let start_idx = state.current_child;
        for offset in 0..children.len() {
            let idx = (start_idx + offset) % children.len();
            let child_status = self.execute_node(children[idx].id, graph, dt);

            match child_status {
                SimNodeStatus::Success => {
                    state.current_child = 0;
                    return SimNodeStatus::Success;
                }
                SimNodeStatus::Running => {
                    return SimNodeStatus::Running;
                }
                SimNodeStatus::Failure => continue,
                _ => {}
            }
        }

        state.current_child = 0;
        SimNodeStatus::Failure
    }''',
'''    fn tick_random_selector(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        
        // Shuffle children order (simulated with state)
        if self.exec_states.get(&node_id.0).unwrap().current_child == 0 {
            // Use a simple pseudo-random based on tick count
            self.exec_states.get_mut(&node_id.0).unwrap().current_child = (self.tick_count as usize) % children.len().max(1);
        }

        let start_idx = self.exec_states.get(&node_id.0).unwrap().current_child;
        for offset in 0..children.len() {
            let idx = (start_idx + offset) % children.len();
            let child_id = children[idx].id;
            let child_status = self.execute_node(child_id, graph, dt);

            match child_status {
                SimNodeStatus::Success => {
                    self.exec_states.get_mut(&node_id.0).unwrap().current_child = 0;
                    return SimNodeStatus::Success;
                }
                SimNodeStatus::Running => {
                    return SimNodeStatus::Running;
                }
                SimNodeStatus::Failure => continue,
                _ => {}
            }
        }

        self.exec_states.get_mut(&node_id.0).unwrap().current_child = 0;
        SimNodeStatus::Failure
    }'''
)

text = text.replace(
'''    fn tick_repeater(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        if children.is_empty() {
            return SimNodeStatus::Failure;
        }

        let state = self.exec_states.get_mut(&node_id.0).unwrap();
        let max_count = graph.nodes.get(&node_id)
            .and_then(|n| n.properties.get("count"))
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(-1); // -1 = infinite

        if max_count >= 0 && state.retry_count >= max_count as u32 {
            state.retry_count = 0;
            return SimNodeStatus::Success;
        }

        let child_status = self.execute_node(children[0].id, graph, dt);
        match child_status {
            SimNodeStatus::Success => {
                state.retry_count += 1;
                if max_count >= 0 && state.retry_count >= max_count as u32 {
                    state.retry_count = 0;
                    SimNodeStatus::Success
                } else {
                    SimNodeStatus::Running
                }
            }
            SimNodeStatus::Failure => {
                state.retry_count = 0;
                SimNodeStatus::Failure
            }
            SimNodeStatus::Running => SimNodeStatus::Running,
            _ => SimNodeStatus::Failure,
        }
    }''',
'''    fn tick_repeater(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        if children.is_empty() {
            return SimNodeStatus::Failure;
        }

        let max_count = graph.nodes.get(&node_id)
            .and_then(|n| n.properties.get("count"))
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(-1); // -1 = infinite

        if max_count >= 0 && self.exec_states.get(&node_id.0).unwrap().retry_count >= max_count as u32 {
            self.exec_states.get_mut(&node_id.0).unwrap().retry_count = 0;
            return SimNodeStatus::Success;
        }

        let child_id = children[0].id;
        let child_status = self.execute_node(child_id, graph, dt);
        
        let state = self.exec_states.get_mut(&node_id.0).unwrap();
        match child_status {
            SimNodeStatus::Success => {
                state.retry_count += 1;
                if max_count >= 0 && state.retry_count >= max_count as u32 {
                    state.retry_count = 0;
                    SimNodeStatus::Success
                } else {
                    SimNodeStatus::Running
                }
            }
            SimNodeStatus::Failure => {
                state.retry_count = 0;
                SimNodeStatus::Failure
            }
            SimNodeStatus::Running => SimNodeStatus::Running,
            _ => SimNodeStatus::Failure,
        }
    }'''
)

text = text.replace(
'''    fn tick_until_success(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        if children.is_empty() {
            return SimNodeStatus::Failure;
        }

        let state = self.exec_states.get_mut(&node_id.0).unwrap();
        let max_count = graph.nodes.get(&node_id)
            .and_then(|n| n.properties.get("max_retries"))
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(-1); // -1 = infinite

        if max_count >= 0 && state.retry_count >= max_count as u32 {
            state.retry_count = 0;
            return SimNodeStatus::Failure;
        }

        let child_status = self.execute_node(children[0].id, graph, dt);
        match child_status {
            SimNodeStatus::Success => {
                state.retry_count = 0;
                SimNodeStatus::Success
            }
            SimNodeStatus::Failure => {
                state.retry_count += 1;
                if max_count >= 0 && state.retry_count >= max_count as u32 {
                    state.retry_count = 0;
                    SimNodeStatus::Failure
                } else {
                    SimNodeStatus::Running
                }
            }
            SimNodeStatus::Running => SimNodeStatus::Running,
            _ => SimNodeStatus::Failure,
        }
    }''',
'''    fn tick_until_success(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        if children.is_empty() {
            return SimNodeStatus::Failure;
        }

        let max_count = graph.nodes.get(&node_id)
            .and_then(|n| n.properties.get("max_retries"))
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(-1); // -1 = infinite

        if max_count >= 0 && self.exec_states.get(&node_id.0).unwrap().retry_count >= max_count as u32 {
            self.exec_states.get_mut(&node_id.0).unwrap().retry_count = 0;
            return SimNodeStatus::Failure;
        }

        let child_id = children[0].id;
        let child_status = self.execute_node(child_id, graph, dt);
        
        let state = self.exec_states.get_mut(&node_id.0).unwrap();
        match child_status {
            SimNodeStatus::Success => {
                state.retry_count = 0;
                SimNodeStatus::Success
            }
            SimNodeStatus::Failure => {
                state.retry_count += 1;
                if max_count >= 0 && state.retry_count >= max_count as u32 {
                    state.retry_count = 0;
                    SimNodeStatus::Failure
                } else {
                    SimNodeStatus::Running
                }
            }
            SimNodeStatus::Running => SimNodeStatus::Running,
            _ => SimNodeStatus::Failure,
        }
    }'''
)

text = text.replace(
'''    fn tick_until_failure(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        if children.is_empty() {
            return SimNodeStatus::Failure;
        }

        let state = self.exec_states.get_mut(&node_id.0).unwrap();
        let max_count = graph.nodes.get(&node_id)
            .and_then(|n| n.properties.get("max_retries"))
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(-1); // -1 = infinite

        if max_count >= 0 && state.retry_count >= max_count as u32 {
            state.retry_count = 0;
            return SimNodeStatus::Failure;
        }

        let child_status = self.execute_node(children[0].id, graph, dt);
        match child_status {
            SimNodeStatus::Failure => {
                state.retry_count = 0;
                SimNodeStatus::Success
            }
            SimNodeStatus::Success => {
                state.retry_count += 1;
                if max_count >= 0 && state.retry_count >= max_count as u32 {
                    state.retry_count = 0;
                    SimNodeStatus::Failure
                } else {
                    SimNodeStatus::Running
                }
            }
            SimNodeStatus::Running => SimNodeStatus::Running,
            _ => SimNodeStatus::Failure,
        }
    }''',
'''    fn tick_until_failure(&mut self, node_id: GraphNodeId, graph: &BtGraphState, dt: f32) -> SimNodeStatus {
        let children = graph.children_of(node_id);
        if children.is_empty() {
            return SimNodeStatus::Failure;
        }

        let max_count = graph.nodes.get(&node_id)
            .and_then(|n| n.properties.get("max_retries"))
            .and_then(|s| s.parse::<i32>().ok())
            .unwrap_or(-1); // -1 = infinite

        if max_count >= 0 && self.exec_states.get(&node_id.0).unwrap().retry_count >= max_count as u32 {
            self.exec_states.get_mut(&node_id.0).unwrap().retry_count = 0;
            return SimNodeStatus::Failure;
        }

        let child_id = children[0].id;
        let child_status = self.execute_node(child_id, graph, dt);
        
        let state = self.exec_states.get_mut(&node_id.0).unwrap();
        match child_status {
            SimNodeStatus::Failure => {
                state.retry_count = 0;
                SimNodeStatus::Success
            }
            SimNodeStatus::Success => {
                state.retry_count += 1;
                if max_count >= 0 && state.retry_count >= max_count as u32 {
                    state.retry_count = 0;
                    SimNodeStatus::Failure
                } else {
                    SimNodeStatus::Running
                }
            }
            SimNodeStatus::Running => SimNodeStatus::Running,
            _ => SimNodeStatus::Failure,
        }
    }'''
)

with open(path, 'w', encoding='utf-8') as f:
    f.write(text)

# Also fix the `get_mut` inside `tick_wait` and `tick_cooldown` if they exist.
