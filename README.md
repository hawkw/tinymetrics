# tinymetrics

a minimal, allocation-free [Prometheus]/[OpenMetrics] metrics implementation for
`no-std` and embedded projects.

[![crates.io][crates-badge]][crates-url]
[![Documentation][docs-badge]][docs-url]
[![Documentation (HEAD)][docs-main-badge]][docs-main-url]
[![MIT licensed][mit-badge]][mit-url]
[![Test Status][tests-badge]][tests-url]
[![Sponsor @hawkw on GitHub Sponsors][sponsor-badge]][sponsor-url]

[crates-badge]: https://img.shields.io/crates/v/tinymetrics.svg
[crates-url]: https://crates.io/crates/tinymetrics
[docs-badge]: https://docs.rs/tinymetrics/badge.svg
[docs-url]: https://docs.rs/tinymetrics
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[mit-url]: ../LICENSE
[tests-badge]: https://github.com/hawkw/tinymetrics/actions/workflows/CI.yml/badge.svg?branch=main
[tests-url]: https://github.com/hawkw/tinymetrics/actions/workflows/CI.yml
[sponsor-badge]: https://img.shields.io/badge/sponsor-%F0%9F%A4%8D-ff69b4
[sponsor-url]: https://github.com/sponsors/hawkw

## why should you use it?

you may want to use this crate if:

1. **you want the [Prometheus]/[OpenMetrics] text exposition format.** other metrics
   systems are not supported. if you want a generic way to record metrics that
   can be emitted in a number of different formats, the i highly recommend the
   [`metrics` crate] and its ecosystem, which provide a generic facade
   implementation that can be used with multiple metrics systems. however, these
   libraries may be less suitable for use in embedded systems &mdash; read on
   for why.
2. **you can't (or don't want to) allocate memory dynamically.** this crate
   is intended to allow all metrics storage to be declared in `static`s, for use
   in embedded systems and other `no-std` use-cases. in order to support
   completely static usage, this crate has some additional limitations over
   other Prometheus/OpenMetrics implementations. in particular:
3. **the cardinality of metrics labels is known ahead of time.** because
   `tinymetrics` stores metrics in `static`, fixed-size arrays, the maximum size
   of the label set of each metric must be declared at compile time. this is an
   inherent limitation to using static storage, but it may be acceptable if you
   only want to expose a small number of metrics with known labels.
4. **you only need [counter] and [gauge] metrics.** i haven't implemented the
   [summary] and [histogram] metric types yet, although it would be nice to
   eventually.

[Prometheus]: https://prometheus.io/
[OpenMetrics]: https://github.com/OpenObservability/OpenMetrics
[`metrics` crate]: https://docs.rs/metrics/
[counter]: https://prometheus.io/docs/concepts/metric_types/#counter
[gauge]: https://prometheus.io/docs/concepts/metric_types/#gauge
[histogram]: https://prometheus.io/docs/concepts/metric_types/#histogram
[summary]: https://prometheus.io/docs/concepts/metric_types/#summary