use std::{collections::HashMap, fmt, sync::Arc};

use tokio::sync::{broadcast, RwLock};
use tracing::{field::Visit, Event, Subscriber};
use tracing_subscriber::{fmt::time::FormatTime, layer::Context, registry::LookupSpan, Layer};

/// A shared registry mapping session IDs to broadcast senders.
/// The `SessionLogRouter` layer uses this to route log events to the correct session stream.
pub type SessionLogSinks = Arc<RwLock<HashMap<usize, broadcast::Sender<String>>>>;

/// Creates a new empty sink map.
pub fn new_log_sinks() -> SessionLogSinks {
    Arc::new(RwLock::new(HashMap::new()))
}

/// A `tracing` layer that inspects span context for a `session` span with an `id` field,
/// and routes formatted log events to the corresponding broadcast channel.
pub struct SessionLogRouter {
    sinks: SessionLogSinks,
}

impl SessionLogRouter {
    pub fn new(sinks: SessionLogSinks) -> Self {
        Self { sinks }
    }
}

impl<S> Layer<S> for SessionLogRouter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // Walk the span scope to find a span named "session" or "session_init" with an "id" field.
        let session_id = ctx
            .event_scope(event)
            .and_then(|scope| {
                for span in scope {
                    let name = span.name();
                    if name.starts_with("session") {
                        let extensions = span.extensions();
                        if let Some(fields) = extensions.get::<SessionSpanFields>() {
                            return Some(fields.id);
                        }
                    }
                }
                None
            })
            .or_else(|| {
                // Fall back to the task-local session ID (set by spawn_with_session).
                contender_core::CURRENT_SESSION_ID.try_with(|id| *id).ok()
            });

        let Some(session_id) = session_id else {
            return;
        };

        // Format the event.
        let formatted = format_event(event, session_id);

        // Try to send non-blocking (don't await the RwLock — use try_read).
        if let Ok(sinks) = self.sinks.try_read() {
            if let Some(tx) = sinks.get(&session_id) {
                let _ = tx.send(formatted);
            }
        }
    }

    fn on_new_span(
        &self,
        attrs: &tracing::span::Attributes<'_>,
        id: &tracing::span::Id,
        ctx: Context<'_, S>,
    ) {
        // When a span named "session*" is created, extract the `id` field and store it.
        let span = ctx.span(id).expect("span not found");
        if span.name().starts_with("session") {
            let mut fields = SessionSpanFields { id: 0 };
            attrs.record(&mut fields);
            span.extensions_mut().insert(fields);
        }
    }
}

/// Stored in span extensions to carry the session ID.
struct SessionSpanFields {
    id: usize,
}

impl Visit for SessionSpanFields {
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if field.name() == "id" {
            self.id = value as usize;
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        if field.name() == "id" {
            self.id = value as usize;
        }
    }

    fn record_debug(&mut self, _field: &tracing::field::Field, _value: &dyn fmt::Debug) {}
}

fn format_event(event: &Event<'_>, session_id: usize) -> String {
    let metadata = event.metadata();
    let mut visitor = MessageVisitor {
        message: String::new(),
    };
    event.record(&mut visitor);

    let mut timestamp = String::new();
    let _ = tracing_subscriber::fmt::time::SystemTime.format_time(
        &mut tracing_subscriber::fmt::format::Writer::new(&mut timestamp),
    );

    format!(
        "{} {} session[{}] {}: {}",
        timestamp,
        metadata.level(),
        session_id,
        metadata.target(),
        visitor.message
    )
}

struct MessageVisitor {
    message: String,
}

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else if !self.message.is_empty() {
            self.message
                .push_str(&format!(" {}={:?}", field.name(), value));
        } else {
            self.message = format!("{}={:?}", field.name(), value);
        }
    }
}
