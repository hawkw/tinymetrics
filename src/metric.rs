use crate::{
    atomic::{AtomicF32, AtomicUsize, Ordering},
    registry::RegistryMap,
};
use core::fmt;

#[cfg(test)]
pub mod tests;

#[derive(Debug)]
pub struct MetricBuilder<'a> {
    name: &'a str,
    help: Option<&'a str>,
    unit: Option<&'a str>,
}

#[derive(Debug)]
pub struct MetricFamily<'a, M, const METRICS: usize> {
    def: MetricBuilder<'a>,
    metrics: RegistryMap<Labels<'a>, M, METRICS>,
}

pub type GaugeFamily<'a, const METRICS: usize> = MetricFamily<'a, Gauge, METRICS>;
pub type CounterFamily<'a, const METRICS: usize> = MetricFamily<'a, Counter, METRICS>;
pub type Labels<'a> = &'a [(&'a str, &'a str)];

pub trait FmtLabels {
    fn fmt_labels(&self, writer: &mut impl fmt::Write) -> fmt::Result;
}

pub trait FmtMetric: Default {
    const TYPE: &'static str;

    fn fmt_metric<F: fmt::Write>(&self, writer: &mut F) -> fmt::Result;
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Gauge {
    value: AtomicF32,
    timestamp: AtomicUsize,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Counter {
    value: AtomicUsize,
    timestamp: AtomicUsize,
}

// === impl MetricDef ===

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

    pub const fn build<M, const METRICS: usize>(self) -> MetricFamily<'a, M, METRICS> {
        MetricFamily {
            def: self,
            metrics: RegistryMap::new(),
        }
    }
}

// === impl MetricFamily ===

impl<'a, M, const METRICS: usize> MetricFamily<'a, M, METRICS>
where
    M: FmtMetric,
{
    pub fn register<'fam>(&'fam self, labels: Labels<'a>) -> Option<&'fam M> {
        self.metrics.get_or_register_default(labels)
    }

    pub fn metrics(&self) -> &RegistryMap<Labels<'a>, M, METRICS> {
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

            let mut labels = labels.iter();
            if let Some(&(k, v)) = labels.next() {
                write!(writer, "{{{k}=\"{v}\"")?;

                for &(k, v) in labels {
                    write!(writer, ",{k}=\"{v}\"")?;
                }

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
            timestamp: AtomicUsize::new(0),
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
            timestamp: AtomicUsize::new(0),
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
