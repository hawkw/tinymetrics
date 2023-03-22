use crate::registry::RegistryMap;
use core::fmt;
use portable_atomic::{AtomicF64, AtomicUsize, Ordering};

#[cfg(feature = "timestamp")]
use crate::timestamp::{TimestampCell, UnixTimestamp};

#[cfg(test)]
mod tests;

/// A builder for constructing [`MetricFamily`] instances.
#[derive(Debug)]
pub struct MetricBuilder<'a> {
    name: &'a str,
    help: &'a str,
    unit: &'a str,
    #[cfg(feature = "timestamp")]
    timestamp_fn: Option<fn() -> UnixTimestamp>,
}

/// An OpenMetrics [MetricFamily].
///
/// A MetricFamily is a collection of metrics points with the same name and metadata.
///
/// [MetricFamily]: https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md#metricfamily
#[derive(Debug)]
pub struct MetricFamily<'a, M, const METRICS: usize, L = LabelSlice<'a>> {
    def: MetricBuilder<'a>,
    metrics: RegistryMap<L, M, METRICS>,
}

pub type GaugeFamily<'a, const METRICS: usize, L = LabelSlice<'a>> =
    MetricFamily<'a, Gauge, METRICS, L>;
pub type CounterFamily<'a, const METRICS: usize, L = LabelSlice<'a>> =
    MetricFamily<'a, Counter, METRICS, L>;
type LabelSlice<'a> = &'a [(&'a str, &'a str)];

/// Trait implemented by types which can be formatted as an OpenMetrics
/// [LabelSet].
///
/// [LabelSet]: https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md#labelset
pub trait FmtLabels {
    fn fmt_labels(&self, writer: &mut impl fmt::Write) -> fmt::Result;

    fn is_empty(&self) -> bool {
        false
    }
}

/// Trait implemented by types which can be formatted as an OpenMetrics
/// [MetricPoint].
///
/// [MetricPoint]: https://github.com/OpenObservability/OpenMetrics/blob/main/specification/OpenMetrics.md#metricpoint
pub trait Metric {
    const TYPE: &'static str;

    fn fmt_metric<F: fmt::Write>(&self, writer: &mut F) -> fmt::Result;

    fn build(builder: &MetricBuilder<'_>) -> Self;
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Gauge {
    value: AtomicF64,

    #[cfg(feature = "timestamp")]
    timestamp: Option<TimestampCell>,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Counter {
    value: AtomicUsize,

    #[cfg(feature = "timestamp")]
    timestamp: Option<TimestampCell>,
}

// === impl FmtLabels ===

impl<L: FmtLabels> FmtLabels for &[L] {
    fn fmt_labels(&self, writer: &mut impl fmt::Write) -> fmt::Result {
        let mut labels = self.iter();
        if let Some(label) = labels.next() {
            label.fmt_labels(writer)?;

            for label in labels {
                writer.write_char(',')?;
                label.fmt_labels(writer)?;
            }
        }

        Ok(())
    }

    fn is_empty(&self) -> bool {
        <[L]>::is_empty(self)
    }
}

impl<L: FmtLabels, const LEN: usize> FmtLabels for [L; LEN] {
    fn fmt_labels(&self, writer: &mut impl fmt::Write) -> fmt::Result {
        (&self[..]).fmt_labels(writer)
    }

    fn is_empty(&self) -> bool {
        LEN > 0
    }
}

impl<K, V> FmtLabels for (K, V)
where
    K: fmt::Display,
    V: fmt::Display,
{
    fn fmt_labels(&self, writer: &mut impl fmt::Write) -> fmt::Result {
        let (k, v) = self;
        write!(writer, "{}=\"{}\"", k, v)
    }

    fn is_empty(&self) -> bool {
        false
    }
}

impl FmtLabels for () {
    fn fmt_labels(&self, _: &mut impl fmt::Write) -> fmt::Result {
        Ok(())
    }

    fn is_empty(&self) -> bool {
        true
    }
}

impl<L: FmtLabels> FmtLabels for &'_ L {
    fn fmt_labels(&self, writer: &mut impl fmt::Write) -> fmt::Result {
        (*self).fmt_labels(writer)
    }

    fn is_empty(&self) -> bool {
        (*self).is_empty()
    }
}

// === impl MetricBuilder ===

impl<'a> MetricBuilder<'a> {
    pub const fn new(name: &'a str) -> Self {
        Self {
            name,
            help: "",
            unit: "",

            #[cfg(all(feature = "std", feature = "timestamp"))]
            timestamp_fn: Some(UnixTimestamp::now),

            #[cfg(all(not(feature = "std"), feature = "timestamp"))]
            timestamp_fn: None,
        }
    }

    pub const fn with_help(self, help: &'a str) -> Self {
        Self { help, ..self }
    }

    pub const fn with_unit(self, unit: &'a str) -> Self {
        Self { unit, ..self }
    }

    #[cfg(feature = "timestamp")]
    pub const fn with_timestamp(self, timestamp_fn: fn() -> UnixTimestamp) -> Self {
        Self {
            timestamp_fn: Some(timestamp_fn),
            ..self
        }
    }

    #[cfg(feature = "timestamp")]
    pub const fn without_timestamps(self) -> Self {
        Self {
            timestamp_fn: None,
            ..self
        }
    }

    #[cfg(feature = "timestamp")]
    const fn mk_timestamp(&self) -> Option<TimestampCell> {
        match self.timestamp_fn {
            Some(t) => Some(TimestampCell::new(t)),
            None => None,
        }
    }

    pub const fn build<M, const METRICS: usize>(self) -> MetricFamily<'a, M, METRICS>
    where
        M: Metric,
    {
        MetricFamily {
            def: self,
            metrics: RegistryMap::new(),
        }
    }

    pub const fn build_labeled<M, L, const METRICS: usize>(self) -> MetricFamily<'a, M, METRICS, L>
    where
        M: Metric,
        L: FmtLabels + PartialEq,
    {
        MetricFamily {
            def: self,
            metrics: RegistryMap::new(),
        }
    }
}

// === impl MetricFamily ===

impl<M, const METRICS: usize, L> MetricFamily<'_, M, METRICS, L> {
    pub fn metrics(&self) -> &RegistryMap<L, M, METRICS> {
        &self.metrics
    }
}

impl<M, L, const METRICS: usize> MetricFamily<'_, M, METRICS, L>
where
    M: Metric,
    L: FmtLabels + PartialEq,
{
    pub fn register(&self, labels: L) -> Option<&M> {
        self.metrics
            .get_or_register_with(labels, || M::build(&self.def))
    }

    pub fn fmt_metric(&self, writer: &mut impl fmt::Write) -> fmt::Result {
        let Self {
            metrics,
            def: MetricBuilder {
                name, help, unit, ..
            },
        } = self;

        writeln!(
            writer,
            "# TYPE {name} {ty}\n# UNIT {name} {unit}\n# HELP {name} {help}",
            ty = M::TYPE
        )?;

        for (labels, metric) in metrics.iter() {
            writer.write_str(name)?;

            if !labels.is_empty() {
                writer.write_char('{')?;
                labels.fmt_labels(writer)?;
                writer.write_char('}')?;
            }

            writer.write_char(' ')?;
            metric.fmt_metric(writer)?;
            writer.write_char('\n')?;
        }
        writer.write_char('\n')?;

        Ok(())
    }
}

impl<M, const METRICS: usize, L> fmt::Display for MetricFamily<'_, M, METRICS, L>
where
    M: Metric,
    L: FmtLabels + PartialEq,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_metric(f)
    }
}

// === impl Gauge ===

impl Gauge {
    const fn from_builder(builder: &MetricBuilder<'_>) -> Self {
        Self {
            value: AtomicF64::new(0.0),
            #[cfg(feature = "timestamp")]
            timestamp: builder.mk_timestamp(),
        }
    }

    pub fn set_value(&self, value: f64) {
        #[cfg(feature = "timestamp")]
        if let Some(ref timestamp) = self.timestamp {
            if !timestamp.update_if_ahead() {
                return;
            }
        }
        self.value.store(value, Ordering::Release);
    }

    pub fn value(&self) -> f64 {
        self.value.load(Ordering::Acquire)
    }
}

impl Metric for Gauge {
    const TYPE: &'static str = "gauge";

    fn fmt_metric<F: fmt::Write>(&self, writer: &mut F) -> fmt::Result {
        write!(writer, "{}", self.value())?;

        #[cfg(feature = "timestamp")]
        if let Some(now) = self.timestamp.as_ref().map(TimestampCell::timestamp) {
            write!(writer, " {now}",)?;
        }

        Ok(())
    }

    fn build(builder: &MetricBuilder<'_>) -> Self {
        Self::from_builder(builder)
    }
}

// === impl Counter ===

impl Counter {
    const fn from_builder(builder: &MetricBuilder<'_>) -> Self {
        Self {
            value: AtomicUsize::new(0),

            #[cfg(feature = "timestamp")]
            timestamp: builder.mk_timestamp(),
        }
    }

    pub fn fetch_add(&self, value: usize) -> usize {
        #[cfg(feature = "timestamp")]
        if let Some(ref timestamp) = self.timestamp {
            timestamp.update_max();
        }
        self.value.fetch_add(value, Ordering::Release)
    }

    pub fn value(&self) -> usize {
        self.value.load(Ordering::Acquire)
    }
}

impl Metric for Counter {
    const TYPE: &'static str = "counter";

    fn fmt_metric<F: fmt::Write>(&self, writer: &mut F) -> fmt::Result {
        write!(writer, "{}", self.value(),)?;

        #[cfg(feature = "timestamp")]
        if let Some(now) = self.timestamp.as_ref().map(TimestampCell::timestamp) {
            write!(writer, " {now}")?;
        }

        Ok(())
    }

    fn build(builder: &MetricBuilder<'_>) -> Self {
        Self::from_builder(builder)
    }
}
