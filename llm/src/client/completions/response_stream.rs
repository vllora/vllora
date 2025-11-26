use std::pin::Pin;

use crate::types::gateway::Chunk;
use async_openai::error::OpenAIError;
use futures::{stream, Stream};

/// Wrapper type around the boxed async stream of raw `Chunk` items.
pub struct ResultStream {
    inner: Pin<Box<dyn Stream<Item = Result<Chunk, OpenAIError>> + Send + 'static>>,
}

impl ResultStream {
    pub fn new(
        inner: Pin<Box<dyn Stream<Item = Result<Chunk, OpenAIError>> + Send + 'static>>,
    ) -> Self {
        Self { inner }
    }

    pub fn create(rx: tokio::sync::mpsc::Receiver<Chunk>) -> ResultStream {
        let response_stream = stream::unfold(rx, |mut receiver| async move {
            receiver.recv().await.map(|chunk| (Ok(chunk), receiver))
        });

        Self::new(Box::pin(response_stream))
    }
}

impl Stream for ResultStream {
    type Item = Result<Chunk, OpenAIError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}
