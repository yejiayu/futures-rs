use super::Stream;
use core::fmt;
use core::marker::PhantomData;
use core::pin::Pin;
use core::task::{Context, Poll};

/// A custom trait object for polling streams, roughly akin to
/// `Box<dyn Stream<Item = T> + 'a>`.
///
/// This custom trait object was introduced for two reasons:
/// - Currently it is not possible to take `dyn Trait` by value and
///   `Box<dyn Trait>` is not available in no_std contexts.
pub struct LocalStreamObj<'a, T> {
    ptr: *mut (),
    poll_next_fn: unsafe fn(*mut (), &mut Context<'_>) -> Poll<Option<T>>,
    drop_fn: unsafe fn(*mut ()),
    _marker: PhantomData<&'a ()>,
}

impl<T> Unpin for LocalStreamObj<'_, T> {}

impl<'a, T> LocalStreamObj<'a, T> {
    /// Create a `LocalStreamObj` from a custom trait object representation.
    #[inline]
    pub fn new<F: UnsafeStreamObj<'a, T> + 'a>(f: F) -> LocalStreamObj<'a, T> {
        LocalStreamObj {
            ptr: f.into_raw(),
            poll_next_fn: F::poll_next,
            drop_fn: F::drop,
            _marker: PhantomData,
        }
    }

    /// Converts the `LocalStreamObj` into a `StreamObj`
    /// To make this operation safe one has to ensure that the `UnsafeStreamObj`
    /// instance from which this `LocalStreamObj` was created actually
    /// implements `Send`.
    #[inline]
    pub unsafe fn into_stream_obj(self) -> StreamObj<'a, T> {
        StreamObj(self)
    }
}

impl<T> fmt::Debug for LocalStreamObj<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalStreamObj").finish()
    }
}

impl<'a, T> From<StreamObj<'a, T>> for LocalStreamObj<'a, T> {
    #[inline]
    fn from(f: StreamObj<'a, T>) -> LocalStreamObj<'a, T> {
        f.0
    }
}

impl<T> Stream for LocalStreamObj<'_, T> {
    type Item = T;

    #[inline]
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<T>> {
        unsafe { (self.poll_next_fn)(self.ptr, cx) }
    }
}

impl<T> Drop for LocalStreamObj<'_, T> {
    fn drop(&mut self) {
        unsafe { (self.drop_fn)(self.ptr) }
    }
}

/// A custom trait object for polling streams, roughly akin to
/// `Box<dyn Stream<Item = T> + Send + 'a>`.
///
/// This custom trait object was introduced for two reasons:
/// - Currently it is not possible to take `dyn Trait` by value and
///   `Box<dyn Trait>` is not available in no_std contexts.
/// - The `Stream` trait is currently not object safe: The `Stream::poll_next`
///   method makes uses the arbitrary self types feature and traits in which
///   this feature is used are currently not object safe due to current compiler
///   limitations. (See tracking issue for arbitray self types for more
///   information #44874)
pub struct StreamObj<'a, T>(LocalStreamObj<'a, T>);

impl<T> Unpin for StreamObj<'_, T> {}
unsafe impl<T> Send for StreamObj<'_, T> {}

impl<'a, T> StreamObj<'a, T> {
    /// Create a `StreamObj` from a custom trait object representation.
    #[inline]
    pub fn new<F: UnsafeStreamObj<'a, T> + Send>(f: F) -> StreamObj<'a, T> {
        StreamObj(LocalStreamObj::new(f))
    }
}

impl<T> fmt::Debug for StreamObj<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StreamObj").finish()
    }
}

impl<T> Stream for StreamObj<'_, T> {
    type Item = T;

    #[inline]
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<T>> {
        let pinned_field = unsafe { Pin::map_unchecked_mut(self, |x| &mut x.0) };
        pinned_field.poll_next(cx)
    }
}

/// A custom implementation of a stream trait object for `StreamObj`, providing
/// a hand-rolled vtable.
///
/// This custom representation is typically used only in `no_std` contexts,
/// where the default `Box`-based implementation is not available.
///
/// The implementor must guarantee that it is safe to call `poll_next`
/// repeatedly (in a non-concurrent fashion) with the result of `into_raw` until
/// `drop` is called.
pub unsafe trait UnsafeStreamObj<'a, T>: 'a {
    /// Convert an owned instance into a (conceptually owned) void pointer.
    fn into_raw(self) -> *mut ();

    /// Poll the stream represented by the given void pointer.
    ///
    /// # Safety
    ///
    /// The trait implementor must guarantee that it is safe to repeatedly call
    /// `poll_next` with the result of `into_raw` until `drop` is called; such
    /// calls are not, however, allowed to race with each other or with calls to
    /// `drop`.
    unsafe fn poll_next(
        ptr: *mut (),
        cx: &mut Context<'_>,
    ) -> Poll<Option<T>>;

    /// Drops the stream represented by the given void pointer.
    ///
    /// # Safety
    ///
    /// The trait implementor must guarantee that it is safe to call this
    /// function once per `into_raw` invocation; that call cannot race with
    /// other calls to `drop` or `poll_next`.
    unsafe fn drop(ptr: *mut ());
}

unsafe impl<'a, T, F> UnsafeStreamObj<'a, T> for &'a mut F
where
    F: Stream<Item = T> + Unpin + 'a,
{
    fn into_raw(self) -> *mut () {
        self as *mut F as *mut ()
    }

    unsafe fn poll_next(
        ptr: *mut (),
        cx: &mut Context<'_>,
    ) -> Poll<Option<T>> {
        Pin::new_unchecked(&mut *(ptr as *mut F)).poll_next(cx)
    }

    unsafe fn drop(_ptr: *mut ()) {}
}

unsafe impl<'a, T, F> UnsafeStreamObj<'a, T> for Pin<&'a mut F>
where
    F: Stream<Item = T> + 'a,
{
    fn into_raw(self) -> *mut () {
        unsafe { Pin::get_unchecked_mut(self) as *mut F as *mut () }
    }

    unsafe fn poll_next(
        ptr: *mut (),
        cx: &mut Context<'_>,
    ) -> Poll<Option<T>> {
        Pin::new_unchecked(&mut *(ptr as *mut F)).poll_next(cx)
    }

    unsafe fn drop(_ptr: *mut ()) {}
}

#[cfg(feature = "alloc")]
mod if_alloc {
    use super::*;
    use core::mem;
    use alloc::boxed::Box;

    unsafe impl<'a, T, F> UnsafeStreamObj<'a, T> for Box<F>
        where F: Stream<Item = T> + 'a
    {
        fn into_raw(self) -> *mut () {
            Box::into_raw(self) as *mut ()
        }

        unsafe fn poll_next(ptr: *mut (), cx: &mut Context<'_>) -> Poll<Option<T>> {
            let ptr = ptr as *mut F;
            let pin: Pin<&mut F> = Pin::new_unchecked(&mut *ptr);
            pin.poll_next(cx)
        }

        unsafe fn drop(ptr: *mut ()) {
            drop(Box::from_raw(ptr as *mut F))
        }
    }

    unsafe impl<'a, T, F> UnsafeStreamObj<'a, T> for Pin<Box<F>>
        where F: Stream<Item = T> + 'a
    {
        fn into_raw(mut self) -> *mut () {
            let mut_ref: &mut F = unsafe { Pin::get_unchecked_mut(self.as_mut()) };
            let ptr = mut_ref as *mut F as *mut ();
            mem::forget(self); // Don't drop the box
            ptr
        }

        unsafe fn poll_next(ptr: *mut (), cx: &mut Context<'_>) -> Poll<Option<T>> {
            let ptr = ptr as *mut F;
            let pin: Pin<&mut F> = Pin::new_unchecked(&mut *ptr);
            pin.poll_next(cx)
        }

        unsafe fn drop(ptr: *mut ()) {
            drop(Box::from_raw(ptr as *mut F))
        }
    }

    impl<'a, F: Stream<Item = ()> + Send + 'a> From<Pin<Box<F>>> for StreamObj<'a, ()> {
        fn from(boxed: Pin<Box<F>>) -> Self {
            StreamObj::new(boxed)
        }
    }

    impl<'a, F: Stream<Item = ()> + Send + 'a> From<Box<F>> for StreamObj<'a, ()> {
        fn from(boxed: Box<F>) -> Self {
            StreamObj::new(boxed)
        }
    }

    impl<'a, F: Stream<Item = ()> + 'a> From<Pin<Box<F>>> for LocalStreamObj<'a, ()> {
        fn from(boxed: Pin<Box<F>>) -> Self {
            LocalStreamObj::new(boxed)
        }
    }

    impl<'a, F: Stream<Item = ()> + 'a> From<Box<F>> for LocalStreamObj<'a, ()> {
        fn from(boxed: Box<F>) -> Self {
            LocalStreamObj::new(boxed)
        }
    }
}
