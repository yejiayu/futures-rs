use futures_core::future::Future;
use futures_core::task::{Context, Poll};
use futures_io::AsyncWrite;
use std::io;
use std::mem;
use std::pin::Pin;

/// Future for the [`write_all`](super::AsyncWriteExt::write_all) method.
#[derive(Debug)]
pub struct WriteAll<'a, W: ?Sized + Unpin> {
    writer: &'a mut W,
    buf: &'a [u8],
}

impl<W: ?Sized + Unpin> Unpin for WriteAll<'_, W> {}

impl<'a, W: AsyncWrite + ?Sized + Unpin> WriteAll<'a, W> {
    pub(super) fn new(writer: &'a mut W, buf: &'a [u8]) -> Self {
        WriteAll { writer, buf }
    }
}

impl<W: AsyncWrite + ?Sized + Unpin> Future for WriteAll<'_, W> {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let this = &mut *self;
        while !this.buf.is_empty() {
            let n = try_ready!(Pin::new(&mut this.writer).poll_write(cx, this.buf));
            {
                let (_, rest) = mem::replace(&mut this.buf, &[]).split_at(n);
                this.buf = rest;
            }
            if n == 0 {
                return Poll::Ready(Err(io::ErrorKind::WriteZero.into()))
            }
        }

        Poll::Ready(Ok(()))
    }
}
