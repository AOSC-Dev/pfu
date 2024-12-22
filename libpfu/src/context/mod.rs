//! Context
//!
//! To apply a lint or fix to a package, callers must prepare a [Context],
//! providing enough information to fixers.
//!
//! Different fixers require different kind of information, for example,
//! the simpliest [crate::lint::src::LintPreferGitSources] linter, requires
//! only [SpecFile] to work. Evaluation of context information
//! can sometimes involve network requests or other heavy calculation,
//! thus, [Context] is designed to be dynamic, leaving all contexts optional.
//!
//! Contexts are [Send] and [Sync], and can be shared across thread.
//!
//! Each context value should be wrapped in a unique struct implementing
//! [ContextValue], and [Context] remembers values according to its type.

use std::{
    any::{Any, TypeId, type_name},
    collections::HashMap,
};

use async_trait::async_trait;
use thiserror::Error;

/// A context including information related to the package to fix.
#[derive(Default)]
pub struct Context {
    /// Safety: the value must be of the same type of its key
    values: HashMap<TypeId, Box<ValueAny>>,
}

type ValueAny = dyn Any + 'static + Send + Sync;

/// A context-related error.
#[derive(Debug, Error)]
pub enum ContextError {
    #[error("Context missing: {0}")]
    MissingValue(&'static str),
}

/// Marker trait for contextual values.
pub trait ContextValue
where
    Self: 'static + Any + Send + Sync,
{
}

impl Context {
    /// Creates a new empty context.
    #[inline]
    pub fn new() -> Self {
        Default::default()
    }

    /// Returns a reference to the internal values map.
    pub fn values(&self) -> &HashMap<TypeId, Box<ValueAny>> {
        &self.values
    }

    /// Returns a mutable reference to the internal values map.
    pub fn values_mut(&mut self) -> &mut HashMap<TypeId, Box<ValueAny>> {
        &mut self.values
    }

    /// Unwraps the context into values.
    pub fn into_values(self) -> HashMap<TypeId, Box<ValueAny>> {
        self.values
    }

    /// Injects a contextual value into the context.
    pub fn inject<V: ContextValue>(&mut self, value: V) -> Result<&mut Self, ContextError> {
        self.values.insert(TypeId::of::<V>(), Box::new(value));
        Ok(self)
    }

    /// Gets a contextual value.
    ///
    /// To get multiple values at once, see [resolve][Context::resolve].
    pub fn get<V: ContextValue>(&self) -> Result<&V, ContextError> {
        self.values
            .get(&TypeId::of::<V>())
            .ok_or(ContextError::MissingValue(type_name::<V>()))
            .map(|value| {
                value
                    .downcast_ref::<V>()
                    .expect("Entry value must be of the same type as its key")
            })
    }

    /// Removes a contextual value.
    pub fn remove<V: ContextValue>(&mut self) -> Option<Box<V>> {
        self.values.remove(&TypeId::of::<V>()).map(|value| {
            value
                .downcast::<V>()
                .expect("Entry value must be of the same type as its key")
        })
    }

    /// Returns `true`` if the context contains nothing.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Clears the context, removing all values.
    pub fn clear(&mut self) {
        self.values.clear();
    }
}

impl<'a> Context {
    /// Resolves some information.
    ///
    /// This is recommended way to get a tuple of values.
    /// For getting a single value, see [get][Context::get].
    pub fn resolve<V: FromContext<'a>>(&'a self) -> Result<V, ContextError> {
        V::from_context(&self)
    }
}

/// A context information resolver.
///
/// Resolving may not be simply getting a value, but may also include
/// conversions, etc.
pub trait FromContext<'a>
where
    Self: Sized,
{
    /// Resolves informations from the context.
    fn from_context(ctx: &'a Context) -> Result<Self, ContextError>;
}

impl<'a, A> FromContext<'a> for &'a A
where
    A: ContextValue,
{
    #[inline]
    fn from_context(ctx: &'a Context) -> Result<Self, ContextError> {
        Ok(ctx.get::<A>()?)
    }
}

impl<'a, A, B> FromContext<'a> for (&'a A, &'a B)
where
    &'a A: FromContext<'a>,
    &'a B: FromContext<'a>,
{
    #[inline]
    fn from_context(ctx: &'a Context) -> Result<Self, ContextError> {
        Ok((
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
        ))
    }
}

impl<'a, A, B, C> FromContext<'a> for (&'a A, &'a B, &'a C)
where
    &'a A: FromContext<'a>,
    &'a B: FromContext<'a>,
    &'a C: FromContext<'a>,
{
    #[inline]
    fn from_context(ctx: &'a Context) -> Result<Self, ContextError> {
        Ok((
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
        ))
    }
}

impl<'a, A, B, C, D> FromContext<'a> for (&'a A, &'a B, &'a C, &'a D)
where
    &'a A: FromContext<'a>,
    &'a B: FromContext<'a>,
    &'a C: FromContext<'a>,
    &'a D: FromContext<'a>,
{
    #[inline]
    fn from_context(ctx: &'a Context) -> Result<Self, ContextError> {
        Ok((
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
        ))
    }
}

impl<'a, A, B, C, D, E> FromContext<'a> for (&'a A, &'a B, &'a C, &'a D, &'a E)
where
    &'a A: FromContext<'a>,
    &'a B: FromContext<'a>,
    &'a C: FromContext<'a>,
    &'a D: FromContext<'a>,
    &'a E: FromContext<'a>,
{
    #[inline]
    fn from_context(ctx: &'a Context) -> Result<Self, ContextError> {
        Ok((
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
        ))
    }
}

impl<'a, A, B, C, D, E, F> FromContext<'a> for (&'a A, &'a B, &'a C, &'a D, &'a E, &'a F)
where
    &'a A: FromContext<'a>,
    &'a B: FromContext<'a>,
    &'a C: FromContext<'a>,
    &'a D: FromContext<'a>,
    &'a E: FromContext<'a>,
    &'a F: FromContext<'a>,
{
    #[inline]
    fn from_context(ctx: &'a Context) -> Result<Self, ContextError> {
        Ok((
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
            FromContext::from_context(ctx)?,
        ))
    }
}

impl<'a, V> FromContext<'a> for Option<&'a V>
where
    &'a V: FromContext<'a>,
{
    #[inline]
    fn from_context(ctx: &'a Context) -> Result<Self, ContextError> {
        match FromContext::from_context(ctx) {
            Ok(val) => Ok(Some(val)),
            Err(ContextError::MissingValue(_)) => Ok(None),
            #[allow(unreachable_patterns)]
            Err(err) => Err(err),
        }
    }
}

/// A provider for one or more context informations.
#[async_trait]
pub trait ContextProvider {
    /// Types of contexts that can be provided.
    fn provides(&self) -> &'static [TypeId];
    /// Attempt to inject contexts.
    async fn inject(&self, ctx: &Context) -> Result<(), ContextError>;
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct TestValA(usize);
    impl ContextValue for TestValA {}

    #[derive(Debug, PartialEq, Eq)]
    struct TestValB(&'static str);
    impl ContextValue for TestValB {}

    #[test]
    fn test_context_basic() {
        let mut ctx = Context::new();
        assert!(ctx.is_empty());
        ctx.get::<TestValA>().unwrap_err();
        ctx.get::<TestValB>().unwrap_err();

        ctx.inject(TestValA(1)).unwrap();
        assert!(!ctx.is_empty());
        assert_eq!(ctx.get::<TestValA>().unwrap(), &TestValA(1));
        ctx.get::<TestValB>().unwrap_err();

        ctx.inject(TestValA(2))
            .unwrap()
            .inject(TestValB("test"))
            .unwrap();
        assert_eq!(ctx.get::<TestValA>().unwrap(), &TestValA(2));
        assert_eq!(ctx.get::<TestValB>().unwrap(), &TestValB("test"));

        ctx.clear();
        assert!(ctx.is_empty());
    }

    #[test]
    fn test_context_resolve() {
        let mut ctx = Context::new();
        ctx.resolve::<(&TestValA, &TestValB)>().unwrap_err();
        ctx.inject(TestValA(1))
            .unwrap()
            .inject(TestValB("test"))
            .unwrap();

        let (TestValA(a), TestValB(b)) = ctx.resolve().unwrap();
        assert_eq!(a, &1);
        assert_eq!(b, &"test");

        let TestValA(a) = ctx.resolve::<&TestValA>().unwrap();
        assert_eq!(a, &1);

        let TestValA(a) = ctx.get().unwrap();
        assert_eq!(a, &1);
    }
}
