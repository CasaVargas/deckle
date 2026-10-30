// SPDX-License-Identifier: GPL-3.0-or-later
//! The app's events (book progress, finished runs) to every open `GET /api/events` stream.

use std::convert::Infallible;
use std::sync::OnceLock;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::Stream;
use tokio::sync::broadcast::{self, error::RecvError};

use crate::Shared;

/// Events nobody has read yet, per stream; a stream further behind skips ahead.
const BACKLOG: usize = 256;

/// One channel for the process: commands emit through the `Ctx` they were given, which the
/// server can't look inside, so the streams listen here instead.
fn channel() -> &'static broadcast::Sender<(String, String)> {
    static CHANNEL: OnceLock<broadcast::Sender<(String, String)>> = OnceLock::new();
    CHANNEL.get_or_init(|| broadcast::channel(BACKLOG).0)
}

/// Sends the app's events to every open `/api/events` stream. Give it to [`deckle_app::dispatch::Ctx`].
pub struct SseEvents;

impl deckle_app::events::Events for SseEvents {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        // No open stream is not an error: nobody is looking.
        let _ = channel().send((event.to_owned(), payload.to_string()));
    }
}

/// `GET /api/events`: `event: <name>` / `data: <json>`, with a keep-alive comment every 15 s.
/// Ends when the server shuts down, so graceful shutdown doesn't wait on it.
pub(crate) async fn stream(
    State(server): State<Shared>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Subscribed here, not in the stream, so nothing emitted after the response is lost.
    let rx = channel().subscribe();
    let stop = server.stop.subscribe();
    // The stream holds the server, and with it the stop sender, for as long as it is open.
    let events = futures_util::stream::unfold(
        (rx, stop, server, false),
        |(mut rx, mut stop, server, mut warned)| async move {
            loop {
                if *stop.borrow_and_update() {
                    return None;
                }
                tokio::select! {
                    got = rx.recv() => match got {
                        Ok((name, data)) => {
                            let event = Event::default().event(name).data(data);
                            return Some((Ok(event), (rx, stop, server, warned)));
                        }
                        Err(RecvError::Lagged(n)) => {
                            if !warned {
                                eprintln!("web UI: an event stream fell behind and skipped {n} events");
                                warned = true;
                            }
                        }
                        Err(RecvError::Closed) => return None,
                    },
                    changed = stop.changed() => if changed.is_err() {
                        return None;
                    },
                }
            }
        },
    );
    Sse::new(events).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
