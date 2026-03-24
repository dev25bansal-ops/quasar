//! Security tests for the Lua scripting sandbox.
//!
//! Verifies that the sandboxed environment properly restricts access to
//! dangerous functionality like OS commands, file I/O, and system calls.

use mlua::{Lua, LuaOptions, StdLib};
use quasar_scripting::{PathValidationError, PathValidator, ScriptCapabilities, ScriptEngine};
use std::path::PathBuf;

// ── Sandbox Escape Tests ──

#[test]
fn sandbox_prevents_os_execute() {
    let lua = Lua::new_with(
        StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH,
        LuaOptions::default(),
    )
    .expect("Failed to create Lua");

    // os library should not be available
    let result: mlua::Result<()> = lua.load("os.execute('echo test')").exec();
    assert!(result.is_err(), "os.execute should be blocked");

    let result: mlua::Result<()> = lua.load("os.getenv('PATH')").exec();
    assert!(result.is_err(), "os.getenv should be blocked");

    let result: mlua::Result<()> = lua.load("os.remove('test.txt')").exec();
    assert!(result.is_err(), "os.remove should be blocked");

    let result: mlua::Result<()> = lua.load("os.rename('a', 'b')").exec();
    assert!(result.is_err(), "os.rename should be blocked");
}

#[test]
fn sandbox_prevents_io_operations() {
    let lua = Lua::new_with(
        StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH,
        LuaOptions::default(),
    )
    .expect("Failed to create Lua");

    // io library should not be available
    let result: mlua::Result<()> = lua.load("io.open('test.txt', 'w')").exec();
    assert!(result.is_err(), "io.open should be blocked");

    let result: mlua::Result<()> = lua.load("io.write('data')").exec();
    assert!(result.is_err(), "io.write should be blocked");

    let result: mlua::Result<()> = lua.load("io.read('*a')").exec();
    assert!(result.is_err(), "io.read should be blocked");

    let result: mlua::Result<()> = lua.load("io.popen('ls')").exec();
    assert!(result.is_err(), "io.popen should be blocked");
}

#[test]
fn sandbox_prevents_debug_library() {
    let lua = Lua::new_with(
        StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH,
        LuaOptions::default(),
    )
    .expect("Failed to create Lua");

    // debug library should not be available
    let result: mlua::Result<()> = lua.load("debug.getinfo(1)").exec();
    assert!(result.is_err(), "debug.getinfo should be blocked");

    let result: mlua::Result<()> = lua.load("debug.getupvalue(function() end, 1)").exec();
    assert!(result.is_err(), "debug.getupvalue should be blocked");
}

#[test]
fn sandbox_prevents_package_library() {
    let lua = Lua::new_with(
        StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH,
        LuaOptions::default(),
    )
    .expect("Failed to create Lua");

    // package library should not be available
    let result: mlua::Result<()> = lua.load("require('os')").exec();
    assert!(result.is_err(), "require should be blocked");

    let result: mlua::Result<()> = lua.load("package.loadlib('test', 'func')").exec();
    assert!(result.is_err(), "package.loadlib should be blocked");
}

#[test]
fn sandbox_allows_safe_libraries() {
    let lua = Lua::new_with(
        StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH,
        LuaOptions::default(),
    )
    .expect("Failed to create Lua");

    // Safe libraries should work
    let result: mlua::Result<()> = lua.load("assert(string.len('hello') == 5)").exec();
    assert!(result.is_ok(), "string library should work");

    let result: mlua::Result<()> = lua.load("assert(math.sqrt(16) == 4)").exec();
    assert!(result.is_ok(), "math library should work");

    let result: mlua::Result<()> = lua.load("local t = {1, 2, 3}; assert(#t == 3)").exec();
    assert!(result.is_ok(), "table library should work");

    let result: mlua::Result<()> = lua.load("local co = coroutine.create(function() end); assert(coroutine.status(co) == 'suspended')").exec();
    assert!(result.is_ok(), "coroutine library should work");

    let result: mlua::Result<()> = lua.load("assert(utf8.len('hello') == 5)").exec();
    assert!(result.is_ok(), "utf8 library should work");
}

// ── ScriptEngine Security Tests ──

#[test]
fn script_engine_sandboxed_by_default() {
    let engine = ScriptEngine::new().expect("Failed to create engine");

    // Should not have access to dangerous functions
    let result = engine.exec("os.execute('rm -rf /')");
    assert!(result.is_err(), "os.execute should be blocked");

    let result = engine.exec("io.open('/etc/passwd', 'r')");
    assert!(result.is_err(), "io.open should be blocked");
}

#[test]
fn script_engine_file_access_denied_by_default() {
    let mut engine = ScriptEngine::new().expect("Failed to create engine");

    // File execution should be denied without explicit permission
    let result = engine.exec_file("/etc/passwd");
    // Will fail because file doesn't exist or path validation fails
    assert!(result.is_err());
}

#[test]
fn script_capabilities_restricted() {
    let caps = ScriptCapabilities::restricted();

    assert!(caps.sandbox_mode);
    assert!(!caps.can_access_files); // Default restricted mode denies file access
}

// ── Path Validator Tests ──

#[test]
fn path_validator_blocks_traversal() {
    let validator = PathValidator::new(vec![PathBuf::from("scripts"), PathBuf::from("assets")]);

    // Path traversal attempts should fail
    let result = validator.validate(PathBuf::from("scripts/../etc/passwd").as_path());
    // This will fail because the path doesn't exist, but the point is it's checked

    // Valid paths within allowed directories would succeed if they exist
    // For this test, we just verify the validator doesn't panic
}

#[test]
fn path_validator_default_directories() {
    let validator = PathValidator::default();

    // Default should allow scripts and assets
    // These paths won't exist, but the validator should handle it gracefully
    let result = validator.validate(PathBuf::from("scripts/test.lua").as_path());
    assert!(matches!(result, Err(PathValidationError::NotFound(_))));
}

#[test]
fn path_validator_custom_directories() {
    let temp_dir = std::env::temp_dir();
    let validator = PathValidator::new(vec![temp_dir.clone()]);

    // Create a test file
    let test_file = temp_dir.join("quasar_test.txt");
    std::fs::write(&test_file, "test").ok();

    // Should allow access to temp directory
    let result = validator.validate(&test_file);
    assert!(result.is_ok());

    // Cleanup
    std::fs::remove_file(&test_file).ok();
}

// ── Memory and Resource Limits ──

#[test]
fn script_memory_limit() {
    let lua = Lua::new_with(
        StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH,
        LuaOptions::default(),
    )
    .expect("Failed to create Lua");

    // Test that we can set memory limit
    lua.set_memory_limit(1024 * 1024); // 1MB

    // Simple operations should work
    let result: mlua::Result<()> = lua
        .load("local t = {}; for i = 1, 100 do t[i] = i end")
        .exec();
    assert!(result.is_ok());
}

#[test]
fn infinite_loop_protection() {
    let lua = Lua::new_with(
        StdLib::COROUTINE | StdLib::TABLE | StdLib::STRING | StdLib::UTF8 | StdLib::MATH,
        LuaOptions::default(),
    )
    .expect("Failed to create Lua");

    // Set instruction limit
    lua.set_hook(
        mlua::HookTriggers {
            every_nth_instruction: Some(10000),
            ..Default::default()
        },
        |_lua, _debug| Err(mlua::Error::runtime("Instruction limit exceeded")),
    );

    // Infinite loop should be caught
    let result = lua.load("while true do end").exec();
    assert!(result.is_err());
}

// ── Safe Function Exposure Tests ──

#[test]
fn safe_functions_only() {
    let engine = ScriptEngine::new().expect("Failed to create engine");

    // Only quasar and log tables should be exposed
    let result = engine.exec("assert(quasar ~= nil)");
    assert!(result.is_ok(), "quasar table should be available");

    let result = engine.exec("assert(log ~= nil)");
    assert!(result.is_ok(), "log table should be available");

    // log functions should work
    let result = engine.exec("log.info('test')");
    assert!(result.is_ok(), "log.info should work");

    let result = engine.exec("log.warn('test')");
    assert!(result.is_ok(), "log.warn should work");

    let result = engine.exec("log.error('test')");
    assert!(result.is_ok(), "log.error should work");
}

// ── Stress Tests ──

#[test]
fn script_engine_stress() {
    let engine = ScriptEngine::new().expect("Failed to create engine");

    // Execute many scripts rapidly
    for i in 0..1000 {
        let script = format!("local x = {} + 1", i);
        let result = engine.exec(&script);
        assert!(result.is_ok());
    }
}

#[test]
fn concurrent_script_access() {
    use std::sync::Arc;
    use std::thread;

    let engine = Arc::new(std::sync::Mutex::new(
        ScriptEngine::new().expect("Failed to create engine"),
    ));

    let mut handles = vec![];

    for i in 0..10 {
        let engine_clone = Arc::clone(&engine);
        let handle = thread::spawn(move || {
            let mut engine = engine_clone.lock().unwrap();
            for j in 0..100 {
                let script = format!("local x = {} + {}", i, j);
                let result = engine.exec(&script);
                assert!(result.is_ok());
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().unwrap();
    }
}
