use std::sync::Arc;

use {Future, IntoFuture, Wake, PollResult, Tokens};
use util;
use chain::Chain;

/// Future for the `and_then` combinator, chaining a computation onto the end of
/// another future which completes successfully.
///
/// This is created by this `Future::and_then` method.
pub struct AndThen<A, B, F> where A: Future, B: IntoFuture {
    state: Chain<A, B::Future, F>,
}

pub fn new<A, B, F>(future: A, f: F) -> AndThen<A, B, F>
    where A: Future,
          B: IntoFuture,
          F: Send + 'static,
{
    AndThen {
        state: Chain::new(future, f),
    }
}

impl<A, B, F> Future for AndThen<A, B, F>
    where A: Future,
          B: IntoFuture<Error=A::Error>,
          F: FnOnce(A::Item) -> B + Send + 'static,
{
    type Item = B::Item;
    type Error = B::Error;

    fn poll(&mut self, tokens: &Tokens)
            -> Option<PollResult<B::Item, B::Error>> {
        self.state.poll(tokens, |result, f| {
            let e = try!(result);
            util::recover(|| f(e)).map(|b| Err(b.into_future()))
        })
    }

    fn schedule(&mut self, wake: Arc<Wake>) {
        self.state.schedule(wake)
    }

    fn tailcall(&mut self)
                -> Option<Box<Future<Item=Self::Item, Error=Self::Error>>> {
        self.state.tailcall()
    }
}