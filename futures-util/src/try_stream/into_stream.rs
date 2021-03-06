use core::pin::Pin;
use futures_core::stream::{FusedStream, Stream, TryStream};
use futures_core::task::{Context, Poll};
use futures_sink::Sink;
use pin_utils::unsafe_pinned;

/// Stream for the [`into_stream`](super::TryStreamExt::into_stream) method.
#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct IntoStream<St> {
    stream: St,
}

impl<St> IntoStream<St> {
    unsafe_pinned!(stream: St);

    #[inline]
    pub(super) fn new(stream: St) -> Self {
        IntoStream { stream }
    }

    /// Acquires a reference to the underlying stream that this combinator is
    /// pulling from.
    pub fn get_ref(&self) -> &St {
        &self.stream
    }

    /// Acquires a mutable reference to the underlying stream that this
    /// combinator is pulling from.
    pub fn get_mut(&mut self) -> &mut St {
        &mut self.stream
    }

    /// Consumes this combinator, returning the underlying stream.
    pub fn into_inner(self) -> St {
        self.stream
    }
}

impl<St: FusedStream> FusedStream for IntoStream<St> {
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated()
    }
}

impl<St: TryStream> Stream for IntoStream<St> {
    type Item = Result<St::Ok, St::Error>;

    #[inline]
    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.stream().try_poll_next(cx)
    }
}

// Forwarding impl of Sink from the underlying stream
impl<S: TryStream + Sink<Item>, Item> Sink<Item> for IntoStream<S> {
    type SinkError = S::SinkError;

    delegate_sink!(stream, Item);
}
