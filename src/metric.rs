use crate::{
    atomic::{AtomicF32, AtomicUsize, Ordering},
    registry::RegistryMap,
};
use core::fmt;

#[cfg(test)]
mod tests;

#[derive(Debug)]
pub struct MetricBuilder<'a> {
    name: &'a str,
    help: Option<&'a str>,
    unit: Option<&'a str>,
}

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

pub trait FmtLabels {
    fn fmt_labels(&self, writer: &mut impl fmt::Write) -> fmt::Result;

    fn is_empty(&self) -> bool {
        false
    }
}

pub trait FmtMetric: Default {
    const TYPE: &'static str;

    fn fmt_metric<F: fmt::Write>(&self, writer: &mut F) -> fmt::Result;
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Gauge {
    value: AtomicF32,
}

#[derive(Debug)]
pub struct Counter {
    value: AtomicUsize,
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
            help: None,
            unit: None,
        }
    }

    pub const fn with_help(self, help: &'a str) -> Self {
        Self {
            help: Some(help),
            ..self
        }
    }

    pub const fn with_unit(self, unit: &'a str) -> Self {
        Self {
            unit: Some(unit),
            ..self
        }
    }

    pub const fn build<M, const METRICS: usize>(self) -> MetricFamily<'a, M, METRICS>
    where
        M: FmtMetric,
    {
        MetricFamily {
            def: self,
            metrics: RegistryMap::new(),
        }
    }

    pub const fn build_labeled<M, L, const METRICS: usize>(self) -> MetricFamily<'a, M, METRICS, L>
    where
        M: FmtMetric,
        L: FmtLabels + PartialEq,
    {
        MetricFamily {
            def: self,
            metrics: RegistryMap::new(),
        }
    }
}

// === impl MetricFamily ===

impl<'a, M, L, const METRICS: usize> MetricFamily<'a, M, METRICS, L>
where
    M: FmtMetric,
    L: FmtLabels + PartialEq,
{
    pub fn register(&self, labels: L) -> Option<&M> {
        self.metrics.get_or_register_default(labels)
    }

    pub fn metrics(&self) -> &RegistryMap<L, M, METRICS> {
        &self.metrics
    }

    pub fn fmt_metric(&self, writer: &mut impl fmt::Write) -> fmt::Result {
        let Self {
            metrics,
            def: MetricBuilder { name, help, unit },
        } = self;

        writeln!(writer, "# TYPE {name} {}", M::TYPE)?;

        if let Some(help) = help {
            writeln!(writer, "# HELP {name} {help}")?;
        }

        if let Some(unit) = unit {
            writeln!(writer, "# UNIT {name} {unit}")?;
        }

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

impl<'a, M, const METRICS: usize> fmt::Display for MetricFamily<'a, M, METRICS>
where
    M: FmtMetric,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_metric(f)
    }
}

// === impl Gauge ===

impl Gauge {
    pub const fn new() -> Self {
        Self {
            value: AtomicF32::zero(),
        }
    }

    pub fn set_value(&self, value: f32) {
        self.value.store(value, Ordering::Release);
    }

    pub fn value(&self) -> f32 {
        self.value.load(Ordering::Acquire)
    }
}

impl FmtMetric for Gauge {
    const TYPE: &'static str = "gauge";

    fn fmt_metric<F: fmt::Write>(&self, writer: &mut F) -> fmt::Result {
        write!(
            writer,
            "{}",
            self.value(),
            // self.timestamp.load(Ordering::Acquire)
        )
    }
}

impl Default for Gauge {
    fn default() -> Self {
        Self::new()
    }
}

// === impl Counter ===

impl Counter {
    pub const fn new() -> Self {
        Self {
            value: AtomicUsize::new(0),
        }
    }

    pub fn fetch_add(&self, value: usize) -> usize {
        self.value.fetch_add(value, Ordering::Release)
    }

    pub fn value(&self) -> usize {
        self.value.load(Ordering::Acquire)
    }
}

impl FmtMetric for Counter {
    const TYPE: &'static str = "counter";

    fn fmt_metric<F: fmt::Write>(&self, writer: &mut F) -> fmt::Result {
        write!(
            writer,
            "{}",
            self.value(),
            // self.timestamp.load(Ordering::Acquire)
        )
    }
}

impl Default for Counter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Counter {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value().serialize(serializer)
    }
}
