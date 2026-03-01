//! Query interface for accessing component data across entities.

use super::{Component, Entity, World};
use std::marker::PhantomData;

/// A query descriptor — currently used as a type-level marker for the
/// component(s) being queried.
///
/// Future iterations will support multi-component queries with tuple syntax:
/// `Query<(&Position, &mut Velocity)>`.
pub struct Query<T: Component> {
    _marker: PhantomData<T>,
}

/// Iterator returned by single-component queries.
pub struct QueryIter<'w, T: Component> {
    inner: Box<dyn Iterator<Item = (Entity, &'w T)> + 'w>,
}

impl<'w, T: Component> Iterator for QueryIter<'w, T> {
    type Item = (Entity, &'w T);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

#[allow(dead_code)]
impl<'w, T: Component> QueryIter<'w, T> {
    pub(crate) fn new(world: &'w World) -> Self {
        Self {
            inner: Box::new(world.query::<T>()),
        }
    }
}
