use axum::{
    extract::State,
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::Stream;
use std::{convert::Infallible, sync::Arc, time::Duration};
use tokio_stream::{wrappers::BroadcastStream, StreamExt as _};

use crate::pkg::realtime::RealtimeHub;

pub async fn sse_handler(
    State(hub): State<Arc<RealtimeHub>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = hub.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(text) => Some(Ok(Event::default().data(text))),
        Err(_) => None,
    });

    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
