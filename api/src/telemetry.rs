use opentelemetry::logs::Logger as _;
use opentelemetry::{logs::LoggerProvider, trace::TracerProvider};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use tracing::{Event, Subscriber};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

struct OpenTelemetryLogLayer {
    logger: opentelemetry_sdk::logs::SdkLogger,
}

impl OpenTelemetryLogLayer {
    fn new(logger: opentelemetry_sdk::logs::SdkLogger) -> Self {
        Self { logger }
    }
}

impl<S> tracing_subscriber::Layer<S> for OpenTelemetryLogLayer
where
    S: Subscriber + for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: tracing_subscriber::layer::Context<'_, S>) {
        use opentelemetry::logs::{LogRecord, Severity};
        use tracing::Level;

        let severity = match *event.metadata().level() {
            Level::ERROR => Severity::Error,
            Level::WARN => Severity::Warn,
            Level::INFO => Severity::Info,
            Level::DEBUG => Severity::Debug,
            Level::TRACE => Severity::Trace,
        };

        struct EventVisitor {
            body: String,
        }

        impl tracing::field::Visit for EventVisitor {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() == "message" {
                    self.body = format!("{:?}", value);
                } else if self.body.is_empty() {
                    self.body = format!("{:?}", value);
                }
            }
        }

        let mut visitor = EventVisitor {
            body: String::new(),
        };
        event.record(&mut visitor);

        let mut log_record = self.logger.create_log_record();
        log_record.set_severity_number(severity);
        log_record.set_severity_text(event.metadata().level().as_str());
        log_record.set_target(event.metadata().target());
        log_record.set_body(visitor.body.into());

        let mut attribute_visitor = std::collections::HashMap::new();
        struct AttributeVisitor<'a>(&'a mut std::collections::HashMap<String, String>);

        impl tracing::field::Visit for AttributeVisitor<'_> {
            fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
                if field.name() != "message" {
                    self.0
                        .insert(field.name().to_string(), format!("{:?}", value));
                }
            }
        }

        event.record(&mut AttributeVisitor(&mut attribute_visitor));

        for (key, value) in attribute_visitor {
            log_record.add_attribute(key, value);
        }
        self.logger.emit(log_record);
    }
}

pub fn init_telemetry_logging() {
    use tracing_subscriber::EnvFilter;
    use tracing_subscriber::filter::LevelFilter;
    use tracing_subscriber::fmt::{self, format::FmtSpan, time::UtcTime};

    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|l| l.parse().ok())
        .unwrap_or(LevelFilter::INFO);

    let telemetry_enabled = std::env::var("TELEMETRY_ENABLED")
        .map(|v| v.trim().to_lowercase() == "true")
        .unwrap_or(false);

    let otel_config = if telemetry_enabled {
        let otel_endpoint = std::env::var("TELEMETRY_ENDPOINT")
            .unwrap_or_else(|_| "http://localhost:4317".to_string());

        println!(
            "Attempting to connect to OpenTelemetry endpoint: {}",
            otel_endpoint
        );

        let span_exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_tonic()
            .with_endpoint(&otel_endpoint)
            .with_protocol(opentelemetry_otlp::Protocol::Grpc)
            .with_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to create OTLP span exporter");

        let log_exporter = opentelemetry_otlp::LogExporter::builder()
            .with_tonic()
            .with_endpoint(&otel_endpoint)
            .with_protocol(opentelemetry_otlp::Protocol::Grpc)
            .with_timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to create OTLP log exporter");

        let tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(span_exporter)
            .with_resource(Resource::builder().with_service_name("psh-ohs").build())
            .build();

        let logger_provider = opentelemetry_sdk::logs::SdkLoggerProvider::builder()
            .with_batch_exporter(log_exporter)
            .with_resource(Resource::builder().with_service_name("psh-ohs").build())
            .build();

        let tracer = tracer_provider.tracer("psh-ohs");
        let logger = logger_provider.logger("psh-ohs");

        println!("OpenTelemetry providers initialized successfully");
        Some((tracer, logger))
    } else {
        println!("OpenTelemetry is disabled, no exporter will be set up");
        None
    };

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.to_string()));

    let fmt_layer = fmt::layer()
        .with_target(true)
        .with_thread_ids(true)
        .with_thread_names(true)
        .with_timer(UtcTime::rfc_3339())
        .with_span_events(FmtSpan::ACTIVE | FmtSpan::CLOSE)
        .with_line_number(true)
        .with_file(true)
        .with_ansi(true)
        .pretty();

    if let Some((tracer, logger)) = otel_config {
        println!("Initializing tracing subscriber with OpenTelemetry");

        let otlp_tracing_layer = OpenTelemetryLayer::new(tracer);
        let otlp_logging_layer = OpenTelemetryLogLayer::new(logger);

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .with(otlp_tracing_layer)
            .with(otlp_logging_layer)
            .try_init()
            .expect("Failed to initialize tracing subscriber with telemetry");
    } else {
        println!("Initializing basic tracing subscriber");

        tracing_subscriber::registry()
            .with(env_filter)
            .with(fmt_layer)
            .try_init()
            .expect("Failed to initialize tracing subscriber");
    }
}
