use super::{fields::CapturedFields, LogLevel, LogRecord, LogRuntime};
use tracing::{
    span::{Attributes, Id, Record},
    Event, Subscriber,
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

#[derive(Clone)]
pub struct ApplicationLogLayer(pub(crate) LogRuntime);

impl<S: Subscriber + for<'a> LookupSpan<'a>> Layer<S> for ApplicationLogLayer {
    fn on_new_span(&self, attributes: &Attributes<'_>, id: &Id, context: Context<'_, S>) {
        let mut fields = CapturedFields::default();
        attributes.record(&mut fields);
        context
            .span(id)
            .expect("registered span")
            .extensions_mut()
            .insert(fields);
    }

    fn on_record(&self, id: &Id, values: &Record<'_>, context: Context<'_, S>) {
        let span = context.span(id).expect("registered span");
        let mut extensions = span.extensions_mut();
        values.record(
            extensions
                .get_mut::<CapturedFields>()
                .expect("captured span fields"),
        );
    }

    fn on_event(&self, event: &Event<'_>, context: Context<'_, S>) {
        let level = match *event.metadata().level() {
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warning,
            tracing::Level::ERROR => LogLevel::Error,
            _ => return,
        };
        let mut fields = CapturedFields::default();
        if let Some(scope) = context.event_scope(event) {
            for span in scope.from_root() {
                if let Some(parent) = span.extensions().get::<CapturedFields>() {
                    fields.values.extend(parent.values.clone());
                    fields.truncated |= parent.truncated;
                }
            }
        }
        event.record(&mut fields);
        let message = fields.values.remove("message").unwrap_or_default();
        let thread = std::thread::current();
        self.0.capture(LogRecord {
            session_id: self.0.session_id().to_owned(),
            sequence: 0,
            timestamp: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            level,
            target: event.metadata().target().chars().take(256).collect(),
            message,
            fields: fields.values,
            thread_id: format!("{:?}", thread.id()),
            thread_name: thread.name().map(|name| name.chars().take(128).collect()),
            truncated: fields.truncated,
        });
    }
}
