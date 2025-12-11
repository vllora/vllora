use std::pin::Pin;

use crate::error::LLMError;
use async_openai::types::responses::ResponseEvent;
use futures::{stream, Stream};

/// Wrapper type around the boxed async stream of raw `Chunk` items.
pub struct ResponsesResultStream {
    inner: Pin<Box<dyn Stream<Item = Result<ResponseEvent, LLMError>> + Send + 'static>>,
}

impl ResponsesResultStream {
    pub fn new(
        inner: Pin<Box<dyn Stream<Item = Result<ResponseEvent, LLMError>> + Send + 'static>>,
    ) -> Self {
        Self { inner }
    }

    pub fn create(
        rx: tokio::sync::mpsc::Receiver<Result<ResponseEvent, LLMError>>,
    ) -> ResponsesResultStream {
        let response_stream = stream::unfold(rx, |mut receiver| async move {
            receiver.recv().await.map(|chunk| (chunk, receiver))
        });

        Self::new(Box::pin(response_stream))
    }
}

impl Stream for ResponsesResultStream {
    type Item = Result<ResponseEvent, LLMError>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}
