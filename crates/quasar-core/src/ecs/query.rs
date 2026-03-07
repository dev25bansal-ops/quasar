//! Query interface for accessing component data across entities.

use super::{Component, Entity, World};
use std::marker::PhantomData;

pub struct Query<T: Component> {
    _marker: PhantomData<T>,
}

pub struct QueryIter<'w, T: Component> {
    inner: std::vec::IntoIter<(Entity, &'w T)>,
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
            inner: world.query::<T>().into_iter(),
        }
    }
}
