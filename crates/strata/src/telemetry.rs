//! The telemetry of `strata api` and `strata worker` (tech design 0133, ADR 0012): the stderr
//! log always, and traces, log records and metrics over OTLP/HTTP when
//! `OTEL_EXPORTER_OTLP_ENDPOINT` is set.

use std::time::Duration;

use opentelemetry::KeyValue;
use opentelemetry::metrics::{Counter, Gauge, Histogram, Meter, UpDownCounter};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tracing_subscriber::layer::{Layer, SubscriberExt};
use tracing_subscriber::{EnvFilter, Registry};

/// The variable that turns the exporters on. The exporters add `/v1/traces`, `/v1/logs` and
/// `/v1/metrics` to it.
pub const ENDPOINT: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";
/// The variable that overrides the service name.
pub const SERVICE_NAME: &str = "OTEL_SERVICE_NAME";
/// The time limit of one export. With three exporters, a stop flushes in less than 5 seconds
/// also when the collector does not answer (TC-133-06).
pub const EXPORT_TIMEOUT: Duration = Duration::from_secs(1);
/// The time limit of the flush at a stop.
pub const FLUSH_TIMEOUT: Duration = Duration::from_secs(3);

/// The filter of `RUST_LOG`, `info` when it is not set: the stderr log of before 0133.
fn rust_log() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into())
}

/// The events of the exporters and of their HTTP client go to stderr only. Otherwise each export
/// makes new spans and log records to export.
fn not_the_exporter(t: &str) -> bool {
    !(t.starts_with("opentelemetry") || t.starts_with("hyper") || t.starts_with("h2"))
}

/// The layers that send the spans and the events of `tracing` to OpenTelemetry.
fn otel_layers<S>(tracer: &SdkTracerProvider, logger: &SdkLoggerProvider) -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    let spans = tracing_opentelemetry::layer()
        .with_tracer(tracer.tracer("strata"))
        .with_filter(rust_log())
        .with_filter(tracing_subscriber::filter::filter_fn(|m| {
            not_the_exporter(m.target())
        }));
    let logs = opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(logger)
        .with_filter(rust_log())
        .with_filter(tracing_subscriber::filter::filter_fn(|m| {
            not_the_exporter(m.target())
        }));
    spans.and_then(logs)
}

fn stderr_layer<S>() -> impl Layer<S>
where
    S: tracing::Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(rust_log())
}

/// The providers of a service that exports. `main` keeps it and calls `shutdown` at a stop.
pub struct Guard {
    exporters: Option<(SdkTracerProvider, SdkLoggerProvider, SdkMeterProvider)>,
}

/// Installs the subscriber of the process. With `export` (`strata api` and `strata worker`) and
/// the endpoint set, it also exports. `service` is the default service name.
pub fn init(service: &str, export: bool) -> Result<Guard, String> {
    let endpoint = std::env::var(ENDPOINT).ok().filter(|e| !e.is_empty());
    if !export || endpoint.is_none() {
        tracing::subscriber::set_global_default(Registry::default().with(stderr_layer()))
            .map_err(|e| e.to_string())?;
        return Ok(Guard { exporters: None });
    };
    let name = std::env::var(SERVICE_NAME).unwrap_or_else(|_| service.to_owned());
    let resource = Resource::builder().with_service_name(name).build();
    let err = |e: opentelemetry_otlp::ExporterBuildError| format!("{ENDPOINT}: {e}");

    let spans = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_timeout(EXPORT_TIMEOUT)
        .build()
        .map_err(err)?;
    use opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor;
    let tracer = SdkTracerProvider::builder()
        .with_resource(resource.clone())
        .with_span_processor(
            BatchSpanProcessor::builder(spans, opentelemetry_sdk::runtime::Tokio).build(),
        )
        .build();

    let logs = opentelemetry_otlp::LogExporter::builder()
        .with_http()
        .with_timeout(EXPORT_TIMEOUT)
        .build()
        .map_err(err)?;
    use opentelemetry_sdk::logs::log_processor_with_async_runtime::BatchLogProcessor;
    let logger = SdkLoggerProvider::builder()
        .with_resource(resource.clone())
        .with_log_processor(
            BatchLogProcessor::builder(ReportErrors(logs), opentelemetry_sdk::runtime::Tokio)
                .build(),
        )
        .build();

    let metrics = opentelemetry_otlp::MetricExporter::builder()
        .with_http()
        .with_timeout(EXPORT_TIMEOUT)
        .build()
        .map_err(err)?;
    use opentelemetry_sdk::metrics::periodic_reader_with_async_runtime::PeriodicReader;
    let meter = SdkMeterProvider::builder()
        .with_resource(resource)
        .with_reader(PeriodicReader::builder(metrics, opentelemetry_sdk::runtime::Tokio).build())
        .build();
    opentelemetry::global::set_meter_provider(meter.clone());

    let subscriber = Registry::default()
        .with(stderr_layer())
        .with(otel_layers(&tracer, &logger));
    tracing::subscriber::set_global_default(subscriber).map_err(|e| e.to_string())?;
    Ok(Guard {
        exporters: Some((tracer, logger, meter)),
    })
}

/// The batch log processor of the SDK drops the error of its periodic export without a word.
/// This wrapper writes it to stderr, as the span and metric processors do.
#[derive(Debug)]
struct ReportErrors<E>(E);

impl<E: opentelemetry_sdk::logs::LogExporter> opentelemetry_sdk::logs::LogExporter
    for ReportErrors<E>
{
    async fn export(
        &self,
        batch: opentelemetry_sdk::logs::LogBatch<'_>,
    ) -> opentelemetry_sdk::error::OTelSdkResult {
        let r = self.0.export(batch).await;
        if let Err(e) = &r {
            tracing::error!(target: "opentelemetry_sdk", error = %e, "the export of log records failed");
        }
        r
    }

    fn shutdown_with_timeout(&self, t: Duration) -> opentelemetry_sdk::error::OTelSdkResult {
        self.0.shutdown_with_timeout(t)
    }

    fn set_resource(&mut self, r: &Resource) {
        self.0.set_resource(r);
    }
}

impl Guard {
    /// Sends the telemetry that the providers hold, in at most `FLUSH_TIMEOUT`. The shutdown of
    /// the SDK blocks, so it runs on its own thread while the runtime serves its export.
    pub async fn shutdown(self) {
        let Some((tracer, logger, meter)) = self.exporters else {
            return;
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        std::thread::spawn(move || {
            for r in [tracer.shutdown(), logger.shutdown(), meter.shutdown()] {
                if let Err(e) = r {
                    eprintln!("telemetry: flush at stop: {e}");
                }
            }
            let _ = tx.send(());
        });
        if tokio::time::timeout(FLUSH_TIMEOUT, rx).await.is_err() {
            eprintln!("telemetry: the flush at stop took more than {FLUSH_TIMEOUT:?}");
        }
    }
}

/// The W3C `traceparent` of a span context: `00-<trace ID>-<span ID>-<flags>`, lowercase hex.
/// `None` for an invalid context, so a job of a request with telemetry off gets null.
pub fn format_traceparent(sc: &opentelemetry::trace::SpanContext) -> Option<String> {
    sc.is_valid().then(|| {
        format!(
            "00-{}-{}-{:02x}",
            sc.trace_id(),
            sc.span_id(),
            sc.trace_flags().to_u8()
        )
    })
}

/// The span context of a `traceparent` of the format of the job row. `None` for any other value,
/// and for an all-zero trace or span ID.
pub fn parse_traceparent(v: &str) -> Option<opentelemetry::trace::SpanContext> {
    use opentelemetry::trace::{SpanContext, SpanId, TraceFlags, TraceId, TraceState};
    let hex = |p: &str, n: usize| {
        p.len() == n && p.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    };
    let [version, trace, span, flags] = v.split('-').collect::<Vec<_>>()[..] else {
        return None;
    };
    if version != "00" || !hex(trace, 32) || !hex(span, 16) || !hex(flags, 2) {
        return None;
    }
    let sc = SpanContext::new(
        TraceId::from_hex(trace).ok()?,
        SpanId::from_hex(span).ok()?,
        TraceFlags::new(u8::from_str_radix(flags, 16).ok()?),
        true,
        TraceState::default(),
    );
    sc.is_valid().then_some(sc)
}

/// The `traceparent` of the current span, for the job that a request queues.
pub fn current_traceparent() -> Option<String> {
    use opentelemetry::trace::TraceContextExt as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    format_traceparent(tracing::Span::current().context().span().span_context())
}

/// Makes `span` a child of `traceparent`. A null or invalid value leaves a root span.
pub fn set_parent(span: &tracing::Span, traceparent: Option<&str>) {
    use opentelemetry::trace::TraceContextExt as _;
    use tracing_opentelemetry::OpenTelemetrySpanExt as _;
    if let Some(sc) = traceparent.and_then(parse_traceparent) {
        let _ = span.set_parent(opentelemetry::Context::new().with_remote_span_context(sc));
    }
}

/// The instruments of tech design 0133. No attribute holds an ID (TC-133-04).
pub struct Metrics {
    request_duration: Histogram<f64>,
    rejections: Counter<u64>,
    in_flight: UpDownCounter<i64>,
    jobs_waiting: Gauge<u64>,
    oldest_waiting_age: Gauge<f64>,
    job_attempts: Counter<u64>,
    extraction_duration: Histogram<f64>,
}

/// The reason of a rejection of the API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rejection {
    Bound,
    RateLimit,
    SearchTimeout,
}

impl Rejection {
    pub fn as_str(self) -> &'static str {
        match self {
            Rejection::Bound => "bound",
            Rejection::RateLimit => "rate_limit",
            Rejection::SearchTimeout => "search_timeout",
        }
    }
}

impl Metrics {
    pub fn new(meter: &Meter) -> Metrics {
        Metrics {
            request_duration: meter
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .build(),
            rejections: meter.u64_counter("strata.api.rejections").build(),
            in_flight: meter
                .i64_up_down_counter("strata.api.requests_in_flight")
                .build(),
            jobs_waiting: meter.u64_gauge("strata.jobs.waiting").build(),
            oldest_waiting_age: meter
                .f64_gauge("strata.jobs.oldest_waiting_age")
                .with_unit("s")
                .build(),
            job_attempts: meter.u64_counter("strata.job.attempts").build(),
            extraction_duration: meter
                .f64_histogram("strata.extraction.duration")
                .with_unit("s")
                .build(),
        }
    }

    /// The instruments of the global meter provider: the OTLP one of `init`, or a no-op one.
    pub fn global() -> std::sync::Arc<Metrics> {
        std::sync::Arc::new(Metrics::new(&opentelemetry::global::meter("strata")))
    }

    pub fn request(&self, method: &str, route: Option<&str>, status: u16, secs: f64) {
        self.request_duration
            .record(secs, &request_attributes(method, route, status));
    }

    pub fn rejection(&self, reason: Rejection) {
        self.rejections
            .add(1, &[KeyValue::new("reason", reason.as_str())]);
    }

    pub fn in_flight(&self, delta: i64) {
        self.in_flight.add(delta, &[]);
    }

    pub fn queue(&self, kind: &'static str, waiting: u64, oldest_age_secs: f64) {
        let a = [KeyValue::new("kind", kind)];
        self.jobs_waiting.record(waiting, &a);
        self.oldest_waiting_age.record(oldest_age_secs, &a);
    }

    pub fn attempt(&self, kind: &'static str, result: &'static str) {
        self.job_attempts.add(1, &attempt_attributes(kind, result));
    }

    pub fn extraction(&self, format: &'static str, result: &'static str, secs: f64) {
        self.extraction_duration
            .record(secs, &extraction_attributes(format, result));
    }
}

fn request_attributes(method: &str, route: Option<&str>, status: u16) -> Vec<KeyValue> {
    let mut a = vec![
        KeyValue::new("http.request.method", method.to_owned()),
        KeyValue::new("http.response.status_code", i64::from(status)),
    ];
    if let Some(r) = route {
        a.push(KeyValue::new("http.route", r.to_owned()));
    }
    a
}

fn attempt_attributes(kind: &'static str, result: &'static str) -> [KeyValue; 2] {
    [KeyValue::new("kind", kind), KeyValue::new("result", result)]
}

fn extraction_attributes(format: &'static str, result: &'static str) -> [KeyValue; 2] {
    [
        KeyValue::new("format", format),
        KeyValue::new("result", result),
    ]
}

/// The in-memory exporters of an L1 test. The test sets `dispatch` as its default subscriber on
/// a current-thread runtime, gives `metrics` to the state, and reads the exporters after
/// `flush`.
#[cfg(feature = "testing")]
pub struct ForTests {
    pub spans: opentelemetry_sdk::trace::InMemorySpanExporter,
    pub logs: opentelemetry_sdk::logs::InMemoryLogExporter,
    pub metric_data: opentelemetry_sdk::metrics::InMemoryMetricExporter,
    pub dispatch: tracing::Dispatch,
    pub metrics: std::sync::Arc<Metrics>,
    tracer: SdkTracerProvider,
    logger: SdkLoggerProvider,
    meter: SdkMeterProvider,
}

#[cfg(feature = "testing")]
pub use opentelemetry_sdk::trace::SpanData;

/// The same layers as `init`, with exporters that write to memory.
#[cfg(feature = "testing")]
pub fn for_tests() -> ForTests {
    use opentelemetry::metrics::MeterProvider as _;
    let spans = opentelemetry_sdk::trace::InMemorySpanExporter::default();
    let logs = opentelemetry_sdk::logs::InMemoryLogExporter::default();
    let metric_data = opentelemetry_sdk::metrics::InMemoryMetricExporter::default();
    let tracer = SdkTracerProvider::builder()
        .with_simple_exporter(spans.clone())
        .build();
    let logger = SdkLoggerProvider::builder()
        .with_simple_exporter(logs.clone())
        .build();
    let meter = SdkMeterProvider::builder()
        .with_reader(
            opentelemetry_sdk::metrics::PeriodicReader::builder(metric_data.clone()).build(),
        )
        .build();
    let dispatch = tracing::Dispatch::new(
        Registry::default()
            .with(stderr_layer())
            .with(otel_layers(&tracer, &logger)),
    );
    ForTests {
        metrics: std::sync::Arc::new(Metrics::new(&meter.meter("strata"))),
        spans,
        logs,
        metric_data,
        dispatch,
        tracer,
        logger,
        meter,
    }
}

#[cfg(feature = "testing")]
impl ForTests {
    /// Exports what the providers hold.
    pub fn flush(&self) {
        let _ = self.tracer.force_flush();
        let _ = self.logger.force_flush();
        let _ = self.meter.force_flush();
    }

    /// The spans that ended.
    pub fn spans(&self) -> Vec<SpanData> {
        self.spans.get_finished_spans().unwrap_or_default()
    }

    /// Each data point of each exported metric: its name and its attributes as text.
    pub fn metric_points(&self) -> Vec<(String, Vec<(String, String)>)> {
        use opentelemetry_sdk::metrics::data::{AggregatedMetrics as A, MetricData as M};
        fn kv<'a>(a: impl Iterator<Item = &'a KeyValue>) -> Vec<(String, String)> {
            a.map(|kv| (kv.key.to_string(), kv.value.to_string()))
                .collect()
        }
        fn points<T>(d: &M<T>) -> Vec<Vec<(String, String)>> {
            match d {
                M::Gauge(g) => g.data_points().map(|p| kv(p.attributes())).collect(),
                M::Sum(s) => s.data_points().map(|p| kv(p.attributes())).collect(),
                M::Histogram(h) => h.data_points().map(|p| kv(p.attributes())).collect(),
                M::ExponentialHistogram(h) => h.data_points().map(|p| kv(p.attributes())).collect(),
            }
        }
        let mut out = Vec::new();
        for rm in self.metric_data.get_finished_metrics().unwrap_or_default() {
            for sm in rm.scope_metrics() {
                for m in sm.metrics() {
                    let pts = match m.data() {
                        A::F64(d) => points(d),
                        A::U64(d) => points(d),
                        A::I64(d) => points(d),
                    };
                    out.extend(pts.into_iter().map(|p| (m.name().to_owned(), p)));
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(a: &[KeyValue]) -> Vec<&str> {
        a.iter().map(|kv| kv.key.as_str()).collect()
    }

    // TC-133-04, L0 half: each attribute set has only the keys of 0133, and none holds an ID.
    #[test]
    fn each_attribute_set_has_only_the_keys_of_the_design() {
        assert_eq!(
            keys(&request_attributes("GET", Some("/v1/search"), 200)),
            [
                "http.request.method",
                "http.response.status_code",
                "http.route"
            ]
        );
        assert_eq!(
            keys(&request_attributes("GET", None, 404)),
            ["http.request.method", "http.response.status_code"]
        );
        assert_eq!(
            keys(&attempt_attributes("ProcessDocument", "Done")),
            ["kind", "result"]
        );
        assert_eq!(
            keys(&extraction_attributes("pdf", "ok")),
            ["format", "result"]
        );
        let reasons = [
            Rejection::Bound,
            Rejection::RateLimit,
            Rejection::SearchTimeout,
        ];
        assert_eq!(
            reasons.map(Rejection::as_str),
            ["bound", "rate_limit", "search_timeout"]
        );
    }

    const VALID: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn a_traceparent_goes_through_parse_and_format() {
        let sc = parse_traceparent(VALID).unwrap();
        assert_eq!(format_traceparent(&sc).as_deref(), Some(VALID));
    }

    #[test]
    fn each_wrong_part_of_a_traceparent_is_refused() {
        for v in [
            "",
            "01-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e473-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-1",
            "00-4BF92F3577B34DA6A3CE929D0E0E4736-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01-00",
            "00-4bf92f3577b34da6a3ce929d0e0e473g-00f067aa0ba902b7-01",
            "00-00000000000000000000000000000000-00f067aa0ba902b7-01",
            "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01",
        ] {
            assert!(parse_traceparent(v).is_none(), "{v}");
        }
    }

    #[test]
    fn no_span_context_gives_no_traceparent() {
        assert!(format_traceparent(&opentelemetry::trace::SpanContext::empty_context()).is_none());
        assert!(current_traceparent().is_none());
    }

    #[test]
    fn the_exporter_events_stay_on_stderr() {
        assert!(!not_the_exporter("opentelemetry_sdk"));
        assert!(!not_the_exporter("opentelemetry-otlp"));
        assert!(!not_the_exporter("hyper_util::client::legacy"));
        assert!(!not_the_exporter("h2::codec"));
        assert!(not_the_exporter("strata::worker"));
    }
}
