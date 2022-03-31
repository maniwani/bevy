use crate::system::{BoxedSystem, NonSend, Res, Resource};

pub type BoxedRunCondition = BoxedSystem<(), bool>;

/// Implemented for [`System`](crate::system::System) types that have read-only data access,
/// have `In=()`, and return `bool`.
pub trait IntoRunCondition<Params>: sealed::IntoRunCondition<Params> {}

impl<Params, F> IntoRunCondition<Params> for F where F: sealed::IntoRunCondition<Params> {}

mod sealed {
    use crate::system::{
        IntoSystem, IsFunctionSystem, ReadOnlySystemParamFetch, SystemParam, SystemParamFunction,
    };

    // This trait is private to prevent implementations for systems that aren't read-only.
    pub trait IntoRunCondition<Params>: IntoSystem<(), bool, Params> {}

    impl<Params, Marker, F> IntoRunCondition<(IsFunctionSystem, Params, Marker)> for F
    where
        F: SystemParamFunction<(), bool, Params, Marker> + Send + Sync + 'static,
        Params: SystemParam + 'static,
        Params::Fetch: ReadOnlySystemParamFetch,
        Marker: 'static,
    {
    }
}

/// Returns `true` if the resource exists.
pub fn resource_exists<T>() -> impl FnMut(Option<Res<T>>) -> bool
where
    T: Resource,
{
    move |res: Option<Res<T>>| res.is_some()
}

/// Returns `true` if the resource is equal to `value`.
///
/// # Panics
///
/// Panics if the resource does not exist.
pub fn resource_equals<T>(value: T) -> impl FnMut(Res<T>) -> bool
where
    T: Resource + PartialEq,
{
    move |res: Res<T>| *res == value
}

/// Returns `true` if the resource exists and is equal to `value`.
pub fn resource_exists_and_equals<T>(value: T) -> impl FnMut(Option<Res<T>>) -> bool
where
    T: Resource + PartialEq,
{
    move |res: Option<Res<T>>| match res {
        Some(res) => *res == value,
        None => false,
    }
}

/// Returns `true` if the non-[`Send`] resource exists.
pub fn non_send_resource_exists<T>() -> impl FnMut(Option<NonSend<T>>) -> bool
where
    T: Resource,
{
    move |res: Option<NonSend<T>>| res.is_some()
}

/// Returns `true` if the non-`Send` resource is equal to `value`.
///
/// # Panics
///
/// Panics if the resource does not exist.
pub fn non_send_resource_equals<T>(value: T) -> impl FnMut(NonSend<T>) -> bool
where
    T: Resource + PartialEq,
{
    move |res: NonSend<T>| *res == value
}

/// Returns `true` if the non-[`Send`] resource exists and is equal to `value`.
pub fn non_send_resource_exists_and_equals<T>(value: T) -> impl FnMut(Option<NonSend<T>>) -> bool
where
    T: Resource + PartialEq,
{
    move |res: Option<NonSend<T>>| match res {
        Some(res) => *res == value,
        None => false,
    }
}
