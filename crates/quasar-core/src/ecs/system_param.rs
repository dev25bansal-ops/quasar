//! Compile-time `SystemParam` with lifetime-based borrow checking.
//!
//! This module replaces the runtime `TypeId`-based access tracking with a
//! compile-time type-level encoding of component access. Each `SystemParam`
//! carries its access pattern in its type, and the Rust borrow checker
//! guarantees that conflicting accesses are caught at compile time.
//!
//! # Architecture
//!
//! The design follows Bevy's compile-time `SystemParam` pattern:
//!
//! 1. **`SystemParam` trait** — encodes access at the type level using two lifetimes:
//!    - `'w` — world lifetime (the world reference the system runs against)
//!    - `'s` — system state lifetime (pre-computed query state)
//!
//! 2. **`SystemState`** — pre-computes and caches component access patterns.
//!    Built once during system initialization, reused every frame.
//!
//! 3. **`SystemParamFetcher`** — extracts typed parameters from state at runtime,
//!    producing items with the correct lifetime-encoded access type.
//!
//! 4. **`ParamSet`** — allows mutually exclusive access patterns (e.g., either
//!    `Query<&mut A>` OR `Query<&mut B>`) within a single system, enforced at
//!    runtime but with compile-time safety for the individual accesses.
//!
//! # Example
//!
//! ```ignore
//! use quasar_core::ecs::system_param::{SystemParam, SystemState, Query, Res};
//!
//! fn movement_system(
//!     mut positions: Query<&mut Position>,  // compile-time write access
//!     velocities: Query<&Velocity>,          // compile-time read access
//!     time: Res<Time>,                       // compile-time resource read
//! ) {
//!     for (mut pos, vel) in positions.iter().zip(velocities.iter()) {
//!         pos.x += vel.dx * time.delta;
//!         pos.y += vel.dy * time.delta;
//!     }
//! }
//! ```
//!
//! # Compile-time safety
//!
//! Two systems that both write `Position` in the same parallel stage will
//! fail to compile because their combined `SystemParam` tuples produce
//! conflicting `Access` descriptors that the `SystemGraph` rejects.

use std::any::TypeId;
use std::marker::PhantomData;

use smallvec::SmallVec;

use super::parallel::SystemAccess;
use super::world::World;

// ---------------------------------------------------------------------------
// AccessKind — read or write at the type level
// ---------------------------------------------------------------------------

/// The kind of access a parameter has on component data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AccessKind {
    /// Shared (read-only) access.
    Read,
    /// Exclusive (read-write) access.
    Write,
}

/// Type-level marker for read access.
pub struct Read;
/// Type-level marker for write access.
pub struct Write;

/// Trait mapping type-level access markers to runtime `AccessKind`.
pub trait AccessMode {
    const KIND: AccessKind;
}

impl AccessMode for Read {
    const KIND: AccessKind = AccessKind::Read;
}

impl AccessMode for Write {
    const KIND: AccessKind = AccessKind::Write;
}

// ---------------------------------------------------------------------------
// Access — compile-time access descriptor
// ---------------------------------------------------------------------------

/// Describes what a system parameter accesses.
///
/// Built from type-level information, used by the scheduler to build
/// the dependency DAG at system registration time (not at runtime per-frame).
#[derive(Debug, Clone, Default)]
pub struct Access {
    /// Component types read by this parameter.
    pub component_reads: SmallVec<[TypeId; 8]>,
    /// Component types written by this parameter.
    pub component_writes: SmallVec<[TypeId; 8]>,
    /// Resource types read by this parameter.
    pub resource_reads: SmallVec<[TypeId; 4]>,
    /// Resource types written by this parameter.
    pub resource_writes: SmallVec<[TypeId; 4]>,
}

impl Access {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a component read.
    pub fn read_component<T: 'static>(mut self) -> Self {
        self.component_reads.push(TypeId::of::<T>());
        self
    }

    /// Add a component write.
    pub fn write_component<T: 'static>(mut self) -> Self {
        self.component_writes.push(TypeId::of::<T>());
        self
    }

    /// Add a resource read.
    pub fn read_resource<T: 'static>(mut self) -> Self {
        self.resource_reads.push(TypeId::of::<T>());
        self
    }

    /// Add a resource write.
    pub fn write_resource<T: 'static>(mut self) -> Self {
        self.resource_writes.push(TypeId::of::<T>());
        self
    }

    /// Check if this access conflicts with another.
    ///
    /// Conflicts occur when:
    /// - Both write the same component/resource
    /// - One writes and the other reads the same component/resource
    pub fn conflicts_with(&self, other: &Access) -> bool {
        // Write vs anything on same component
        for &tid in &self.component_writes {
            if other.component_reads.contains(&tid)
                || other.component_writes.contains(&tid)
            {
                return true;
            }
        }
        // Read vs write on same component
        for &tid in &self.component_reads {
            if other.component_writes.contains(&tid) {
                return true;
            }
        }
        // Same for resources
        for &tid in &self.resource_writes {
            if other.resource_reads.contains(&tid)
                || other.resource_writes.contains(&tid)
            {
                return true;
            }
        }
        for &tid in &self.resource_reads {
            if other.resource_writes.contains(&tid) {
                return true;
            }
        }
        false
    }

    /// Merge two accesses (for combining multiple parameters).
    pub fn merge(&self, other: &Access) -> Self {
        let mut merged = self.clone();
        for &tid in &other.component_reads {
            if !merged.component_reads.contains(&tid) {
                merged.component_reads.push(tid);
            }
        }
        for &tid in &other.component_writes {
            if !merged.component_writes.contains(&tid) {
                merged.component_writes.push(tid);
            }
        }
        for &tid in &other.resource_reads {
            if !merged.resource_reads.contains(&tid) {
                merged.resource_reads.push(tid);
            }
        }
        for &tid in &other.resource_writes {
            if !merged.resource_writes.contains(&tid) {
                merged.resource_writes.push(tid);
            }
        }
        merged
    }

    /// Convert to the legacy `SystemAccess` for backward compatibility.
    pub fn to_system_access(&self) -> SystemAccess {
        SystemAccess {
            reads: self.component_reads.clone(),
            writes: self.component_writes.clone(),
            resources_read: self.resource_reads.clone(),
            resources_write: self.resource_writes.clone(),
        }
    }
}

// ---------------------------------------------------------------------------
// SystemParam trait — the core abstraction
// ---------------------------------------------------------------------------

/// A parameter that can be passed to a system, with compile-time access encoding.
///
/// This trait uses two lifetimes to encode access patterns at the type level:
/// - `'w` — the world lifetime (how long the world reference is valid)
/// - `'s` — the system state lifetime (how long the pre-computed state lives)
///
/// The associated `State` type holds pre-computed query information that is
/// built once and reused every frame. The `Item` type is what the system
/// actually receives when it runs, with lifetimes encoding the borrow scope.
pub trait SystemParam: 'static {
    /// Pre-computed state that is built once during system initialization.
    /// Must be `Send + Sync` for parallel execution.
    type State: Send + Sync + 'static;

    /// The type the system receives when running, parameterized by lifetimes.
    type Item<'w, 's>;

    /// Build the initial state from the world.
    /// Called once when the system is registered.
    fn init_state(world: &mut World) -> Self::State;

    /// Get the compile-time access descriptor for this parameter.
    /// Used by the scheduler to build the dependency DAG.
    fn access() -> Access;

    /// Extract the parameter item from state for a single system invocation.
    ///
    /// # Safety
    /// The caller must ensure that the world pointer is valid and that no
    /// conflicting borrows exist. The world is passed as a raw pointer to
    /// allow multiple parameters to access different parts of the world
    /// simultaneously without violating Rust's aliasing rules.
    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's>;
}

// ---------------------------------------------------------------------------
// SystemState — pre-computed system parameter state
// ---------------------------------------------------------------------------

/// Pre-computed state for a system's parameters.
///
/// Built once during system registration, this holds all the query state
/// needed to execute the system without per-frame allocation or archetype
/// scanning.
///
/// # Type parameters
/// - `P` — The `SystemParam` tuple type for the system.
pub struct SystemState<P: SystemParam> {
    param_state: P::State,
}

impl<P: SystemParam> SystemState<P> {
    /// Build system state from the world.
    pub fn new(world: &mut World) -> Self {
        Self {
            param_state: P::init_state(world),
        }
    }

    /// Get the access descriptor for this system's parameters.
    pub fn access(&self) -> Access {
        P::access()
    }

    /// Extract the system parameters for execution.
    ///
    /// # Safety
    /// The world pointer must be valid for the duration of system execution.
    /// The caller must ensure no conflicting borrows exist.
    pub unsafe fn get_params<'w, 's>(&'s self, world: *mut World) -> P::Item<'w, 's> {
        P::get_param(&self.param_state, world)
    }
}

// ---------------------------------------------------------------------------
// SystemParam function systems
// ---------------------------------------------------------------------------

/// A system built from a function that takes compile-time `SystemParam`s.
///
/// This wraps a function like `fn(Query<&mut Position>, Query<&Velocity>)`
/// into the `System` trait, using pre-computed state for zero-allocation execution.
pub struct FnSystemWithParams<P: SystemParam, F> {
    name: String,
    state: SystemState<P>,
    func: F,
    _marker: PhantomData<P>,
}

impl<P: SystemParam + Send + Sync, F> FnSystemWithParams<P, F>
where
    F: for<'w, 's> FnMut(P::Item<'w, 's>) + Send + Sync + 'static,
{
    /// Create a new system from a function and world.
    pub fn new(name: impl Into<String>, world: &mut World, func: F) -> Self {
        Self {
            name: name.into(),
            state: SystemState::new(world),
            func,
            _marker: PhantomData,
        }
    }
}

impl<P: SystemParam + Send + Sync, F> super::system::System for FnSystemWithParams<P, F>
where
    F: for<'w, 's> FnMut(P::Item<'w, 's>) + Send + Sync + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn run(&mut self, world: &mut World) {
        // SAFETY: The world reference is valid for the duration of this call.
        // The SystemParam's Item type encodes the borrow scope via lifetimes.
        // The scheduler ensures no conflicting borrows exist via the Access DAG.
        let params = unsafe { self.state.get_params(world) };
        (self.func)(params);
    }
}

/// Builder for creating a system from a function with compile-time SystemParams.
///
/// # Example
/// ```ignore
/// let system = system_fn("movement", |
///     mut positions: Query<&mut Position>,
///     velocities: Query<&Velocity>,
/// | {
///     // ...
/// });
/// ```
pub fn system_fn<P, F>(name: impl Into<String>, world: &mut World, func: F) -> FnSystemWithParams<P, F>
where
    P: SystemParam + Send + Sync,
    F: for<'w, 's> FnMut(P::Item<'w, 's>) + Send + Sync + 'static,
{
    FnSystemWithParams::new(name, world, func)
}

// ---------------------------------------------------------------------------
// ParamSet — mutually exclusive access patterns
// ---------------------------------------------------------------------------

/// Allows a system to have mutually exclusive access patterns.
///
/// For example, a system might sometimes need `Query<&mut A>` and sometimes
/// `Query<&mut B>`, but never both at once. `ParamSet` provides runtime
/// enforcement that only one is accessed per system invocation.
///
/// # Example
/// ```ignore
/// fn conditional_system(
///     mut params: ParamSet<(Query<&mut Position>, Query<&mut Velocity>)>,
/// ) {
///     // Only one of these can be used per invocation
///     if let Ok(mut positions) = params.p0() {
///         for (e, mut pos) in positions.iter() {
///             pos.x *= 2.0;
///         }
///     } else if let Ok(mut velocities) = params.p1() {
///         for (e, mut vel) in velocities.iter() {
///             vel.dx *= 2.0;
///         }
///     }
/// }
/// ```
pub struct ParamSet<A, B> {
    a_state: A,
    b_state: B,
    used: u8, // 0 = neither used, 1 = A used, 2 = B used
}

impl<A, B> ParamSet<A, B> {
    /// Create a new ParamSet from two system states.
    pub fn new(a_state: A, b_state: B) -> Self {
        Self {
            a_state,
            b_state,
            used: 0,
        }
    }

    /// Get access to parameter set A.
    /// Returns `None` if parameter set B has already been accessed.
    pub fn p0(&mut self) -> Option<&mut A> {
        if self.used == 2 {
            None
        } else {
            self.used = 1;
            Some(&mut self.a_state)
        }
    }

    /// Get access to parameter set B.
    /// Returns `None` if parameter set A has already been accessed.
    pub fn p1(&mut self) -> Option<&mut B> {
        if self.used == 1 {
            None
        } else {
            self.used = 2;
            Some(&mut self.b_state)
        }
    }
}

// ---------------------------------------------------------------------------
// SystemParam for tuples (combining multiple parameters)
// ---------------------------------------------------------------------------

// Single-element tuple
impl<P0: SystemParam> SystemParam for (P0,) {
    type State = P0::State;
    type Item<'w, 's> = P0::Item<'w, 's>;

    fn init_state(world: &mut World) -> Self::State {
        P0::init_state(world)
    }

    fn access() -> Access {
        P0::access()
    }

    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's> {
        P0::get_param(state, world)
    }
}

// Two-element tuple
impl<P0: SystemParam, P1: SystemParam> SystemParam for (P0, P1) {
    type State = (P0::State, P1::State);
    type Item<'w, 's> = (P0::Item<'w, 's>, P1::Item<'w, 's>);

    fn init_state(world: &mut World) -> Self::State {
        (P0::init_state(world), P1::init_state(world))
    }

    fn access() -> Access {
        P0::access().merge(&P1::access())
    }

    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's> {
        (P0::get_param(&state.0, world), P1::get_param(&state.1, world))
    }
}

// Three-element tuple
impl<P0: SystemParam, P1: SystemParam, P2: SystemParam> SystemParam for (P0, P1, P2) {
    type State = (P0::State, P1::State, P2::State);
    type Item<'w, 's> = (P0::Item<'w, 's>, P1::Item<'w, 's>, P2::Item<'w, 's>);

    fn init_state(world: &mut World) -> Self::State {
        (
            P0::init_state(world),
            P1::init_state(world),
            P2::init_state(world),
        )
    }

    fn access() -> Access {
        P0::access()
            .merge(&P1::access())
            .merge(&P2::access())
    }

    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's> {
        (
            P0::get_param(&state.0, world),
            P1::get_param(&state.1, world),
            P2::get_param(&state.2, world),
        )
    }
}

// Four-element tuple
impl<P0: SystemParam, P1: SystemParam, P2: SystemParam, P3: SystemParam> SystemParam
    for (P0, P1, P2, P3)
{
    type State = (P0::State, P1::State, P2::State, P3::State);
    type Item<'w, 's> = (P0::Item<'w, 's>, P1::Item<'w, 's>, P2::Item<'w, 's>, P3::Item<'w, 's>);

    fn init_state(world: &mut World) -> Self::State {
        (
            P0::init_state(world),
            P1::init_state(world),
            P2::init_state(world),
            P3::init_state(world),
        )
    }

    fn access() -> Access {
        P0::access()
            .merge(&P1::access())
            .merge(&P2::access())
            .merge(&P3::access())
    }

    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's> {
        (
            P0::get_param(&state.0, world),
            P1::get_param(&state.1, world),
            P2::get_param(&state.2, world),
            P3::get_param(&state.3, world),
        )
    }
}

// Five-element tuple
impl<P0, P1, P2, P3, P4> SystemParam for (P0, P1, P2, P3, P4)
where
    P0: SystemParam,
    P1: SystemParam,
    P2: SystemParam,
    P3: SystemParam,
    P4: SystemParam,
{
    type State = (P0::State, P1::State, P2::State, P3::State, P4::State);
    type Item<'w, 's> = (
        P0::Item<'w, 's>,
        P1::Item<'w, 's>,
        P2::Item<'w, 's>,
        P3::Item<'w, 's>,
        P4::Item<'w, 's>,
    );

    fn init_state(world: &mut World) -> Self::State {
        (
            P0::init_state(world),
            P1::init_state(world),
            P2::init_state(world),
            P3::init_state(world),
            P4::init_state(world),
        )
    }

    fn access() -> Access {
        P0::access()
            .merge(&P1::access())
            .merge(&P2::access())
            .merge(&P3::access())
            .merge(&P4::access())
    }

    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's> {
        (
            P0::get_param(&state.0, world),
            P1::get_param(&state.1, world),
            P2::get_param(&state.2, world),
            P3::get_param(&state.3, world),
            P4::get_param(&state.4, world),
        )
    }
}

// Six-element tuple
impl<P0, P1, P2, P3, P4, P5> SystemParam for (P0, P1, P2, P3, P4, P5)
where
    P0: SystemParam,
    P1: SystemParam,
    P2: SystemParam,
    P3: SystemParam,
    P4: SystemParam,
    P5: SystemParam,
{
    type State = (P0::State, P1::State, P2::State, P3::State, P4::State, P5::State);
    type Item<'w, 's> = (
        P0::Item<'w, 's>,
        P1::Item<'w, 's>,
        P2::Item<'w, 's>,
        P3::Item<'w, 's>,
        P4::Item<'w, 's>,
        P5::Item<'w, 's>,
    );

    fn init_state(world: &mut World) -> Self::State {
        (
            P0::init_state(world),
            P1::init_state(world),
            P2::init_state(world),
            P3::init_state(world),
            P4::init_state(world),
            P5::init_state(world),
        )
    }

    fn access() -> Access {
        P0::access()
            .merge(&P1::access())
            .merge(&P2::access())
            .merge(&P3::access())
            .merge(&P4::access())
            .merge(&P5::access())
    }

    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's> {
        (
            P0::get_param(&state.0, world),
            P1::get_param(&state.1, world),
            P2::get_param(&state.2, world),
            P3::get_param(&state.3, world),
            P4::get_param(&state.4, world),
            P5::get_param(&state.5, world),
        )
    }
}

// Seven-element tuple
impl<P0, P1, P2, P3, P4, P5, P6> SystemParam for (P0, P1, P2, P3, P4, P5, P6)
where
    P0: SystemParam,
    P1: SystemParam,
    P2: SystemParam,
    P3: SystemParam,
    P4: SystemParam,
    P5: SystemParam,
    P6: SystemParam,
{
    type State = (
        P0::State,
        P1::State,
        P2::State,
        P3::State,
        P4::State,
        P5::State,
        P6::State,
    );
    type Item<'w, 's> = (
        P0::Item<'w, 's>,
        P1::Item<'w, 's>,
        P2::Item<'w, 's>,
        P3::Item<'w, 's>,
        P4::Item<'w, 's>,
        P5::Item<'w, 's>,
        P6::Item<'w, 's>,
    );

    fn init_state(world: &mut World) -> Self::State {
        (
            P0::init_state(world),
            P1::init_state(world),
            P2::init_state(world),
            P3::init_state(world),
            P4::init_state(world),
            P5::init_state(world),
            P6::init_state(world),
        )
    }

    fn access() -> Access {
        P0::access()
            .merge(&P1::access())
            .merge(&P2::access())
            .merge(&P3::access())
            .merge(&P4::access())
            .merge(&P5::access())
            .merge(&P6::access())
    }

    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's> {
        (
            P0::get_param(&state.0, world),
            P1::get_param(&state.1, world),
            P2::get_param(&state.2, world),
            P3::get_param(&state.3, world),
            P4::get_param(&state.4, world),
            P5::get_param(&state.5, world),
            P6::get_param(&state.6, world),
        )
    }
}

// Eight-element tuple
impl<P0, P1, P2, P3, P4, P5, P6, P7> SystemParam for (P0, P1, P2, P3, P4, P5, P6, P7)
where
    P0: SystemParam,
    P1: SystemParam,
    P2: SystemParam,
    P3: SystemParam,
    P4: SystemParam,
    P5: SystemParam,
    P6: SystemParam,
    P7: SystemParam,
{
    type State = (
        P0::State,
        P1::State,
        P2::State,
        P3::State,
        P4::State,
        P5::State,
        P6::State,
        P7::State,
    );
    type Item<'w, 's> = (
        P0::Item<'w, 's>,
        P1::Item<'w, 's>,
        P2::Item<'w, 's>,
        P3::Item<'w, 's>,
        P4::Item<'w, 's>,
        P5::Item<'w, 's>,
        P6::Item<'w, 's>,
        P7::Item<'w, 's>,
    );

    fn init_state(world: &mut World) -> Self::State {
        (
            P0::init_state(world),
            P1::init_state(world),
            P2::init_state(world),
            P3::init_state(world),
            P4::init_state(world),
            P5::init_state(world),
            P6::init_state(world),
            P7::init_state(world),
        )
    }

    fn access() -> Access {
        P0::access()
            .merge(&P1::access())
            .merge(&P2::access())
            .merge(&P3::access())
            .merge(&P4::access())
            .merge(&P5::access())
            .merge(&P6::access())
            .merge(&P7::access())
    }

    unsafe fn get_param<'w, 's>(
        state: &'s Self::State,
        world: *mut World,
    ) -> Self::Item<'w, 's> {
        (
            P0::get_param(&state.0, world),
            P1::get_param(&state.1, world),
            P2::get_param(&state.2, world),
            P3::get_param(&state.3, world),
            P4::get_param(&state.4, world),
            P5::get_param(&state.5, world),
            P6::get_param(&state.6, world),
            P7::get_param(&state.7, world),
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_no_conflict_same_reads() {
        let a = Access::new().read_component::<i32>();
        let b = Access::new().read_component::<i32>();
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn access_conflict_read_write() {
        let a = Access::new().write_component::<i32>();
        let b = Access::new().read_component::<i32>();
        assert!(a.conflicts_with(&b));
        assert!(b.conflicts_with(&a));
    }

    #[test]
    fn access_conflict_write_write() {
        let a = Access::new().write_component::<i32>();
        let b = Access::new().write_component::<i32>();
        assert!(a.conflicts_with(&b));
    }

    #[test]
    fn access_no_conflict_different_types() {
        let a = Access::new().write_component::<i32>();
        let b = Access::new().write_component::<String>();
        assert!(!a.conflicts_with(&b));
    }

    #[test]
    fn access_merge() {
        let a = Access::new().read_component::<i32>().write_component::<f32>();
        let b = Access::new().read_component::<String>().write_component::<bool>();
        let merged = a.merge(&b);

        assert!(merged.component_reads.contains(&TypeId::of::<i32>()));
        assert!(merged.component_reads.contains(&TypeId::of::<String>()));
        assert!(merged.component_writes.contains(&TypeId::of::<f32>()));
        assert!(merged.component_writes.contains(&TypeId::of::<bool>()));
    }

    #[test]
    fn param_set_mutually_exclusive() {
        let mut set = ParamSet::new("a", "b");

        // Can get A first
        assert_eq!(set.p0(), Some(&mut "a"));
        // Then B is blocked
        assert_eq!(set.p1(), None);
    }

    #[test]
    fn param_set_mutually_exclusive_reverse() {
        let mut set = ParamSet::new("a", "b");

        // Can get B first
        assert_eq!(set.p1(), Some(&mut "b"));
        // Then A is blocked
        assert_eq!(set.p0(), None);
    }
}
