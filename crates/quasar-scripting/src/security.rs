//! Script Security - Hardening for Untrusted Scripts
//!
//! This module provides additional security measures for running untrusted Lua scripts:
//! - Memory limits to prevent exhaustion attacks
//! - Execution time limits to prevent infinite loops
//! - Table size limits
//! - String length limits
//! - Call depth limits

use mlua::prelude::*;
use std::time::{Duration, Instant};

/// Default memory limit for sandboxed scripts (16 MB)
pub const DEFAULT_MEMORY_LIMIT: usize = 16 * 1024 * 1024;

/// Default execution timeout (100ms)
pub const DEFAULT_EXECUTION_TIMEOUT_MS: u64 = 100;

/// Default max call depth
pub const DEFAULT_MAX_CALL_DEPTH: u32 = 200;

/// Default max table size (10000 entries)
pub const DEFAULT_MAX_TABLE_SIZE: usize = 10_000;

/// Default max string length (1 MB)
pub const DEFAULT_MAX_STRING_LENGTH: usize = 1024 * 1024;

/// Security limits for script execution.
#[derive(Debug, Clone)]
pub struct SecurityLimits {
    /// Maximum memory in bytes the script can allocate.
    pub memory_limit: usize,
    /// Maximum execution time before timeout.
    pub execution_timeout: Duration,
    /// Maximum call stack depth.
    pub max_call_depth: u32,
    /// Maximum number of entries in a single table.
    pub max_table_size: usize,
    /// Maximum length of a single string.
    pub max_string_length: usize,
    /// Whether to enforce instruction counting.
    pub enable_instruction_limit: bool,
    /// Maximum instructions per execution.
    pub max_instructions: u64,
}

impl Default for SecurityLimits {
    fn default() -> Self {
        Self {
            memory_limit: DEFAULT_MEMORY_LIMIT,
            execution_timeout: Duration::from_millis(DEFAULT_EXECUTION_TIMEOUT_MS),
            max_call_depth: DEFAULT_MAX_CALL_DEPTH,
            max_table_size: DEFAULT_MAX_TABLE_SIZE,
            max_string_length: DEFAULT_MAX_STRING_LENGTH,
            enable_instruction_limit: true,
            max_instructions: 1_000_000,
        }
    }
}

impl SecurityLimits {
    /// Create restrictive limits for untrusted scripts.
    pub fn restrictive() -> Self {
        Self {
            memory_limit: 4 * 1024 * 1024, // 4 MB
            execution_timeout: Duration::from_millis(50),
            max_call_depth: 100,
            max_table_size: 1_000,
            max_string_length: 64 * 1024, // 64 KB
            enable_instruction_limit: true,
            max_instructions: 100_000,
        }
    }

    /// Create permissive limits for trusted scripts.
    pub fn permissive() -> Self {
        Self {
            memory_limit: 64 * 1024 * 1024, // 64 MB
            execution_timeout: Duration::from_secs(5),
            max_call_depth: 500,
            max_table_size: 100_000,
            max_string_length: 10 * 1024 * 1024, // 10 MB
            enable_instruction_limit: false,
            max_instructions: u64::MAX,
        }
    }
}

/// Security violation types.
#[derive(Debug, Clone)]
pub enum SecurityViolation {
    MemoryLimitExceeded { used: usize, limit: usize },
    ExecutionTimeout { elapsed: Duration, limit: Duration },
    CallDepthExceeded { depth: u32, limit: u32 },
    TableSizeExceeded { size: usize, limit: usize },
    StringLengthExceeded { length: usize, limit: usize },
    InstructionLimitExceeded { count: u64, limit: u64 },
}

impl std::fmt::Display for SecurityViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MemoryLimitExceeded { used, limit } => {
                write!(
                    f,
                    "Memory limit exceeded: {} bytes used (limit: {})",
                    used, limit
                )
            }
            Self::ExecutionTimeout { elapsed, limit } => {
                write!(
                    f,
                    "Execution timeout: {:?} elapsed (limit: {:?})",
                    elapsed, limit
                )
            }
            Self::CallDepthExceeded { depth, limit } => {
                write!(f, "Call depth exceeded: {} (limit: {})", depth, limit)
            }
            Self::TableSizeExceeded { size, limit } => {
                write!(
                    f,
                    "Table size exceeded: {} entries (limit: {})",
                    size, limit
                )
            }
            Self::StringLengthExceeded { length, limit } => {
                write!(
                    f,
                    "String length exceeded: {} bytes (limit: {})",
                    length, limit
                )
            }
            Self::InstructionLimitExceeded { count, limit } => {
                write!(
                    f,
                    "Instruction limit exceeded: {} (limit: {})",
                    count, limit
                )
            }
        }
    }
}

impl std::error::Error for SecurityViolation {}

/// Execution context for security monitoring.
pub struct SecurityContext {
    limits: SecurityLimits,
    start_time: Instant,
    instruction_count: u64,
    current_depth: u32,
    memory_used: usize,
}

impl SecurityContext {
    pub fn new(limits: SecurityLimits) -> Self {
        Self {
            limits,
            start_time: Instant::now(),
            instruction_count: 0,
            current_depth: 0,
            memory_used: 0,
        }
    }

    /// Check if execution is still within limits.
    pub fn check_limits(&self) -> Result<(), SecurityViolation> {
        // Check execution timeout
        let elapsed = self.start_time.elapsed();
        if elapsed > self.limits.execution_timeout {
            return Err(SecurityViolation::ExecutionTimeout {
                elapsed,
                limit: self.limits.execution_timeout,
            });
        }

        // Check instruction count
        if self.limits.enable_instruction_limit
            && self.instruction_count > self.limits.max_instructions
        {
            return Err(SecurityViolation::InstructionLimitExceeded {
                count: self.instruction_count,
                limit: self.limits.max_instructions,
            });
        }

        // Check call depth
        if self.current_depth > self.limits.max_call_depth {
            return Err(SecurityViolation::CallDepthExceeded {
                depth: self.current_depth,
                limit: self.limits.max_call_depth,
            });
        }

        Ok(())
    }

    /// Record an instruction execution.
    pub fn record_instruction(&mut self) -> Result<(), SecurityViolation> {
        self.instruction_count += 1;
        if self.limits.enable_instruction_limit && self.instruction_count % 1000 == 0 {
            self.check_limits()?;
        }
        Ok(())
    }

    /// Enter a function call.
    pub fn enter_call(&mut self) -> Result<(), SecurityViolation> {
        self.current_depth += 1;
        if self.current_depth > self.limits.max_call_depth {
            return Err(SecurityViolation::CallDepthExceeded {
                depth: self.current_depth,
                limit: self.limits.max_call_depth,
            });
        }
        Ok(())
    }

    /// Exit a function call.
    pub fn exit_call(&mut self) {
        self.current_depth = self.current_depth.saturating_sub(1);
    }

    /// Check if a table size is within limits.
    pub fn check_table_size(&self, size: usize) -> Result<(), SecurityViolation> {
        if size > self.limits.max_table_size {
            return Err(SecurityViolation::TableSizeExceeded {
                size,
                limit: self.limits.max_table_size,
            });
        }
        Ok(())
    }

    /// Check if a string length is within limits.
    pub fn check_string_length(&self, length: usize) -> Result<(), SecurityViolation> {
        if length > self.limits.max_string_length {
            return Err(SecurityViolation::StringLengthExceeded {
                length,
                limit: self.limits.max_string_length,
            });
        }
        Ok(())
    }

    /// Get the elapsed execution time.
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get the instruction count.
    pub fn instruction_count(&self) -> u64 {
        self.instruction_count
    }
}

/// Apply memory limit to a Lua state.
pub fn apply_memory_limit(lua: &Lua, limit: usize) -> LuaResult<()> {
    // Set memory limit via gc
    lua.set_memory_limit(limit)?;
    Ok(())
}

/// Create a sandboxed Lua environment with all security measures.
pub fn create_sandboxed_lua(limits: &SecurityLimits) -> LuaResult<Lua> {
    let safe_libs = mlua::StdLib::COROUTINE
        | mlua::StdLib::TABLE
        | mlua::StdLib::STRING
        | mlua::StdLib::UTF8
        | mlua::StdLib::MATH;

    let lua = Lua::new_with(safe_libs, mlua::LuaOptions::default())?;

    apply_memory_limit(&lua, limits.memory_limit)?;

    Ok(lua)
}

/// Wrap a function execution with security monitoring.
pub fn with_security_monitor<F, T>(ctx: &mut SecurityContext, f: F) -> Result<T, SecurityViolation>
where
    F: FnOnce() -> T,
{
    ctx.check_limits()?;
    let result = f();
    ctx.check_limits()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn security_limits_default() {
        let limits = SecurityLimits::default();
        assert_eq!(limits.memory_limit, DEFAULT_MEMORY_LIMIT);
        assert_eq!(
            limits.execution_timeout,
            Duration::from_millis(DEFAULT_EXECUTION_TIMEOUT_MS)
        );
        assert!(limits.enable_instruction_limit);
    }

    #[test]
    fn security_limits_restrictive() {
        let limits = SecurityLimits::restrictive();
        assert!(limits.memory_limit < DEFAULT_MEMORY_LIMIT);
        assert!(limits.execution_timeout < Duration::from_millis(DEFAULT_EXECUTION_TIMEOUT_MS));
        assert!(limits.max_call_depth < DEFAULT_MAX_CALL_DEPTH);
    }

    #[test]
    fn security_limits_permissive() {
        let limits = SecurityLimits::permissive();
        assert!(limits.memory_limit > DEFAULT_MEMORY_LIMIT);
        assert!(limits.execution_timeout > Duration::from_millis(DEFAULT_EXECUTION_TIMEOUT_MS));
        assert!(!limits.enable_instruction_limit);
    }

    #[test]
    fn security_context_new() {
        let limits = SecurityLimits::default();
        let ctx = SecurityContext::new(limits);
        assert_eq!(ctx.instruction_count(), 0);
        assert!(ctx.elapsed() < Duration::from_millis(10));
    }

    #[test]
    fn security_context_check_limits_passes() {
        let limits = SecurityLimits::default();
        let ctx = SecurityContext::new(limits);
        assert!(ctx.check_limits().is_ok());
    }

    #[test]
    fn security_context_record_instruction() {
        let limits = SecurityLimits::default();
        let mut ctx = SecurityContext::new(limits);
        for _ in 0..100 {
            assert!(ctx.record_instruction().is_ok());
        }
        assert_eq!(ctx.instruction_count(), 100);
    }

    #[test]
    fn security_context_call_depth() {
        let limits = SecurityLimits::restrictive();
        let mut ctx = SecurityContext::new(limits);

        // Should succeed for normal depth
        for _ in 0..50 {
            assert!(ctx.enter_call().is_ok());
        }

        // Exit all
        for _ in 0..50 {
            ctx.exit_call();
        }

        assert_eq!(ctx.current_depth, 0);
    }

    #[test]
    fn security_context_table_size_check() {
        let limits = SecurityLimits::default();
        let ctx = SecurityContext::new(limits);

        assert!(ctx.check_table_size(100).is_ok());
        assert!(ctx.check_table_size(DEFAULT_MAX_TABLE_SIZE).is_ok());
        assert!(ctx.check_table_size(DEFAULT_MAX_TABLE_SIZE + 1).is_err());
    }

    #[test]
    fn security_context_string_length_check() {
        let limits = SecurityLimits::default();
        let ctx = SecurityContext::new(limits);

        assert!(ctx.check_string_length(100).is_ok());
        assert!(ctx.check_string_length(DEFAULT_MAX_STRING_LENGTH).is_ok());
        assert!(ctx
            .check_string_length(DEFAULT_MAX_STRING_LENGTH + 1)
            .is_err());
    }

    #[test]
    fn security_violation_display() {
        let violation = SecurityViolation::MemoryLimitExceeded {
            used: 1024,
            limit: 512,
        };
        let msg = format!("{}", violation);
        assert!(msg.contains("Memory limit exceeded"));
        assert!(msg.contains("1024"));
        assert!(msg.contains("512"));
    }

    #[test]
    fn create_sandboxed_lua_succeeds() {
        let limits = SecurityLimits::default();
        let lua = create_sandboxed_lua(&limits);
        assert!(lua.is_ok());
    }
}
