use std::{any::type_name, marker::PhantomData};

use crate::{
    query::{Query, QueryData, QueryFilter, filter::EmptyQueryFilter},
    system::{
        SystemAccessMeta, SystemParam,
        system_error::{Severity, SystemParamError},
    },
    world::World,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuerySingleError {
    NoMatches,
    MultipleMatches,
}

impl std::fmt::Display for QuerySingleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMatches => {
                write!(
                    f,
                    "QuerySingleError\n\
                    Type: NoMatches\n\
                    The query expected exactly one matching entity, but found none."
                )
            }
            Self::MultipleMatches => {
                write!(
                    f,
                    "QuerySingleError\n\
                    Type: MultipleMatches\n\
                    The query expected exactly one matching entity, but found more than one."
                )
            }
        }
    }
}

/// A system parameter that provides read-only access to a single, unique entity's component data.
///
/// This parameter fetches a specific entity matching the requested `QueryData` and `QueryFilter`.
/// If there is not exactly one entity matching the criteria within the current frame, the host system
/// automatically skips its execution entirely.
pub struct Single<'w, Q: QueryData, F: QueryFilter = EmptyQueryFilter> {
    item: Q::ReadOnlyItem<'w>,
    _query: Query<'w, Q, F>,
    _marker: PhantomData<F>,
}

impl<'w, Q: QueryData, F: QueryFilter> SystemParam for Single<'w, Q, F> {
    type Item<'world> = Single<'world, Q, F>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        Query::<Q, F>::init_access(system_meta);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        let query = <Query<Q, F> as SystemParam>::get_param(world)?;
        let query = unsafe { std::mem::transmute::<Query<'_, Q, F>, Query<'world, Q, F>>(query) };

        let result = match query.get_single() {
            Ok(item_ref) => {
                let item = unsafe {
                    std::mem::transmute::<Q::ReadOnlyItem<'_>, Q::ReadOnlyItem<'world>>(item_ref)
                };
                Ok(item)
            }
            Err(err @ QuerySingleError::NoMatches) => Err(SystemParamError::new(
                type_name::<Self>(),
                Severity::Error,
                err.to_string(),
                true,
            )),
            Err(err @ QuerySingleError::MultipleMatches) => Err(SystemParamError::new(
                type_name::<Self>(),
                Severity::Error,
                err.to_string(),
                true,
            )),
        };

        let item = result?;

        Ok(Single {
            item,
            _query: query,
            _marker: PhantomData,
        })
    }
}

impl<'w, Q: QueryData, F: QueryFilter> std::ops::Deref for Single<'w, Q, F> {
    type Target = Q::ReadOnlyItem<'w>;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<'w, Q: QueryData, F: QueryFilter> std::ops::DerefMut for Single<'w, Q, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

/// A system parameter that provides mutable write access to a single, unique entity's component data.
///
/// This parameter fetches a specific entity matching the requested `QueryData` and `QueryFilter`.
/// If there is not exactly one entity matching the criteria within the current frame, the host system
/// automatically skips its execution entirely.
pub struct SingleMut<'w, Q: QueryData, F: QueryFilter = EmptyQueryFilter> {
    item: Q::Item<'w>,
    _query: Query<'w, Q, F>,
    _marker: PhantomData<F>,
}

impl<'w, Q: QueryData, F: QueryFilter> SystemParam for SingleMut<'w, Q, F> {
    type Item<'world> = SingleMut<'world, Q, F>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        Query::<Q, F>::init_access(system_meta);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        let query = <Query<Q, F> as SystemParam>::get_param(world)?;
        let mut query =
            unsafe { std::mem::transmute::<Query<'_, Q, F>, Query<'world, Q, F>>(query) };

        let result = match query.get_single_mut() {
            Ok(item_ref) => {
                let item = unsafe { std::mem::transmute::<Q::Item<'_>, Q::Item<'world>>(item_ref) };
                Ok(item)
            }
            Err(err @ QuerySingleError::NoMatches) => Err(SystemParamError::new(
                type_name::<Self>(),
                Severity::Error,
                err.to_string(),
                true,
            )),
            Err(err @ QuerySingleError::MultipleMatches) => Err(SystemParamError::new(
                type_name::<Self>(),
                Severity::Error,
                err.to_string(),
                true,
            )),
        };

        let item = result?;

        Ok(SingleMut {
            item,
            _query: query,
            _marker: PhantomData,
        })
    }
}

impl<'w, Q: QueryData, F: QueryFilter> std::ops::Deref for SingleMut<'w, Q, F> {
    type Target = Q::Item<'w>;

    fn deref(&self) -> &Self::Target {
        &self.item
    }
}

impl<'w, Q: QueryData, F: QueryFilter> std::ops::DerefMut for SingleMut<'w, Q, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.item
    }
}

impl<'w, Q: QueryData, F: QueryFilter> SystemParam for Option<Single<'w, Q, F>> {
    type Item<'world> = Option<Single<'world, Q, F>>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        Query::<Q, F>::init_access(system_meta);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        let query = <Query<Q, F> as SystemParam>::get_param(world)?;
        let query = unsafe { std::mem::transmute::<Query<'_, Q, F>, Query<'world, Q, F>>(query) };

        let result = match query.get_single() {
            Ok(item_ref) => {
                let item = unsafe {
                    std::mem::transmute::<Q::ReadOnlyItem<'_>, Q::ReadOnlyItem<'world>>(item_ref)
                };

                Ok(Some(item))
            }
            Err(QuerySingleError::NoMatches) => Ok(None),
            Err(QuerySingleError::MultipleMatches) => Ok(None),
        };

        let item_opt = result?;

        Ok(item_opt.map(|item| Single {
            item,
            _query: query,
            _marker: PhantomData,
        }))
    }
}

impl<'w, Q: QueryData, F: QueryFilter> SystemParam for Option<SingleMut<'w, Q, F>> {
    type Item<'world> = Option<SingleMut<'world, Q, F>>;

    fn init_access(system_meta: &mut SystemAccessMeta) {
        Query::<Q, F>::init_access(system_meta);
    }

    fn get_param<'world>(world: &'world World) -> Result<Self::Item<'world>, SystemParamError> {
        let query = <Query<Q, F> as SystemParam>::get_param(world)?;
        let mut query =
            unsafe { std::mem::transmute::<Query<'_, Q, F>, Query<'world, Q, F>>(query) };

        let result = match query.get_single_mut() {
            Ok(item_ref) => {
                let item = unsafe { std::mem::transmute::<Q::Item<'_>, Q::Item<'world>>(item_ref) };
                Ok(Some(item))
            }
            Err(QuerySingleError::NoMatches) => Ok(None),
            Err(QuerySingleError::MultipleMatches) => Ok(None),
        };

        let item_opt = result?;

        Ok(item_opt.map(|item| SingleMut {
            item,
            _query: query,
            _marker: PhantomData,
        }))
    }
}
