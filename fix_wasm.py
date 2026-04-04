import re

# Read the file
with open(
    "C:/Users/dev25/Projects/quasar/crates/quasar-scripting/src/wasm_ecs_bridge.rs",
    "r",
    encoding="utf-8",
) as f:
    content = f.read()

# Define the old pattern
old_pattern = """                    // Wait for the host to set the ID
                    // In practice, this is set after command processing
                    let id = loop {
                        let val = result_id.lock().unwrap().clone();
                        if val != 0 {
                            break val;
                        }
                        std::thread::yield_now();
                    };
                    id"""

# Define the new replacement
new_code = """                    // Wait for the host to set the ID with timeout protection
                    const SPAWN_TIMEOUT_SECS: u64 = 5;
                    let start = std::time::Instant::now();
                    let id = loop {
                        let val = match result_id.lock() {
                            Ok(guard) => *guard,
                            Err(e) => {
                                log::warn!("Lock poisoned in spawn_entity: {}", e);
                                0
                            }
                        };
                        if val != 0 {
                            break val;
                        }
                        if start.elapsed().as_secs() >= SPAWN_TIMEOUT_SECS {
                            log::error!("spawn_entity timeout - host did not set entity ID within {} seconds", SPAWN_TIMEOUT_SECS);
                            return 0;
                        }
                        std::thread::yield_now();
                    };
                    id"""

# Perform the replacement
if old_pattern in content:
    content = content.replace(old_pattern, new_code)
    print("Fixed infinite loop issue")
else:
    print("Pattern not found")

# Write back
with open(
    "C:/Users/dev25/Projects/quasar/crates/quasar-scripting/src/wasm_ecs_bridge.rs",
    "w",
    encoding="utf-8",
) as f:
    f.write(content)
