use super::*;
use pretty_assertions::assert_str_eq;

#[test]
fn gauge() {
    let family = {
        let builder = MetricBuilder::new("test_gauge")
            .with_help("a test gauge")
            .with_unit("tests");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Gauge, 2>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.set_value(10.0);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.set_value(22.2);

    let expected = "\
    # TYPE test_gauge gauge\n\
    # UNIT test_gauge tests\n\
    # HELP test_gauge a test gauge\n\
    test_gauge{metric=\"1\",label2=\"foo\"} 10\n\
    test_gauge{metric=\"2\",label2=\"bar\"} 22.2\n\n\
    ";
    assert_str_eq!(family.to_string(), expected);
}

#[test]
fn counter() {
    let family = {
        let builder = MetricBuilder::new("test_counter")
            .with_help("a test counter")
            .with_unit("tests");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Counter, 2>()
    };

    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.fetch_add(1);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.fetch_add(2);

    let expected = "\
    # TYPE test_counter counter\n\
    # UNIT test_counter tests\n\
    # HELP test_counter a test counter\n\
    test_counter{metric=\"1\",label2=\"foo\"} 1\n\
    test_counter{metric=\"2\",label2=\"bar\"} 2\n\n\
    ";
    assert_str_eq!(family.to_string(), expected);
}

#[test]
fn gauges_dont_start_at_0_if_unrecorded() {
    let family = {
        let builder = MetricBuilder::new("test_gauge")
            .with_help("a test gauge")
            .with_unit("tests");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Gauge, 2>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");

    let expected = "\
        # TYPE test_gauge gauge\n\
        # UNIT test_gauge tests\n\
        # HELP test_gauge a test gauge\n\
        \n\
    ";
    assert_str_eq!(family.to_string(), expected);

    metric1.set_value(10.0);

    let expected = "\
        # TYPE test_gauge gauge\n\
        # UNIT test_gauge tests\n\
        # HELP test_gauge a test gauge\n\
        test_gauge{metric=\"1\",label2=\"foo\"} 10\n\
        \n\
    ";
    assert_str_eq!(family.to_string(), expected);

    metric2.set_value(5.0);

    let expected = "\
        # TYPE test_gauge gauge\n\
        # UNIT test_gauge tests\n\
        # HELP test_gauge a test gauge\n\
        test_gauge{metric=\"1\",label2=\"foo\"} 10\n\
        test_gauge{metric=\"2\",label2=\"bar\"} 5\n\
        \n\
    ";
    assert_str_eq!(family.to_string(), expected);
}

#[test]
#[cfg(feature = "timestamp")]
fn gauge_timestamped() {
    use portable_atomic::{AtomicU64, Ordering};
    static NOW: AtomicU64 = AtomicU64::new(100);

    let family = MetricBuilder::new("test_gauge")
        .with_help("a test gauge")
        .with_unit("tests")
        .with_timestamp(|| crate::UnixTimestamp::from_secs(NOW.load(Ordering::SeqCst)))
        .build::<Gauge, 2>();
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.set_value(10.0);

    // advance the clock
    NOW.store(200, Ordering::SeqCst);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.set_value(22.2);

    let expected = "\
    # TYPE test_gauge gauge\n\
    # UNIT test_gauge tests\n\
    # HELP test_gauge a test gauge\n\
    test_gauge{metric=\"1\",label2=\"foo\"} 10 100\n\
    test_gauge{metric=\"2\",label2=\"bar\"} 22.2 200\n\n\
    ";
    assert_str_eq!(family.to_string(), expected);
}

#[test]
#[cfg(feature = "timestamp")]
fn counter_timestamped() {
    use portable_atomic::{AtomicU64, Ordering};
    static NOW: AtomicU64 = AtomicU64::new(100);

    let family = MetricBuilder::new("test_counter")
        .with_help("a test counter")
        .with_unit("tests")
        .with_timestamp(|| crate::UnixTimestamp::from_secs(NOW.load(Ordering::SeqCst)))
        .build::<Counter, 2>();

    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.fetch_add(1);

    // advance the clock
    NOW.store(150, Ordering::SeqCst);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.fetch_add(1);

    // advance the clock again
    NOW.store(200, Ordering::SeqCst);
    metric2.fetch_add(1);

    let expected = "\
    # TYPE test_counter counter\n\
    # UNIT test_counter tests\n\
    # HELP test_counter a test counter\n\
    test_counter{metric=\"1\",label2=\"foo\"} 1 100\n\
    test_counter{metric=\"2\",label2=\"bar\"} 2 200\n\n\
    ";
    assert_str_eq!(family.to_string(), expected);
}

#[test]
fn gauge_min() {
    let family = {
        let builder = MetricBuilder::new("test_gauge");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Gauge, 3>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.set_value(10.0);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.set_value(22.2);

    let metric3 = family
        .register(&[("metric", "3"), ("label2", "baz")])
        .expect("metric 2 must register");
    metric3.set_value(33.3);

    assert_eq!(family.min_value(), Some(10.0));
}

#[test]
fn gauge_max() {
    let family = {
        let builder = MetricBuilder::new("test_gauge");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Gauge, 3>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.set_value(10.0);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.set_value(22.2);

    let metric3 = family
        .register(&[("metric", "3"), ("label2", "baz")])
        .expect("metric 2 must register");
    metric3.set_value(33.3);

    assert_eq!(family.max_value(), Some(33.3));
}

#[test]
fn gauge_total() {
    let family = {
        let builder = MetricBuilder::new("test_gauge");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Gauge, 4>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.set_value(10.0);

    // register an unrecorded metric to ensure that it doesn't get added to the
    // total.
    let _metric2 = family
        .register(&[("metric", "unrecorded"), ("label2", "lol")])
        .expect("metric 2 must register");

    let metric3 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric3.set_value(22.0);

    let metric4 = family
        .register(&[("metric", "3"), ("label2", "baz")])
        .expect("metric 2 must register");
    metric4.set_value(33.0);

    assert_eq!(family.total(), 65.0);
}

#[test]
fn gauge_mean() {
    let family = {
        let builder = MetricBuilder::new("test_gauge");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Gauge, 4>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.set_value(2.0);

    // register an unrecorded metric to ensure that it doesn't get added to the
    // mean.
    let _metric2 = family
        .register(&[("metric", "unrecorded"), ("label2", "lol")])
        .expect("metric 2 must register");

    let metric3 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 3 must register");
    metric3.set_value(5.0);

    let metric4 = family
        .register(&[("metric", "3"), ("label2", "baz")])
        .expect("metric 4 must register");
    metric4.set_value(14.0);

    assert_eq!(family.mean(), Some(7.0));
}

#[test]
fn counter_min() {
    let family = {
        let builder = MetricBuilder::new("test_counter");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Counter, 3>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.fetch_add(10);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.fetch_add(22);

    let metric3 = family
        .register(&[("metric", "3"), ("label2", "baz")])
        .expect("metric 2 must register");
    metric3.fetch_add(33);

    assert_eq!(family.min_value(), Some(10));
}

#[test]
fn counter_max() {
    let family = {
        let builder = MetricBuilder::new("test_counter");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Counter, 3>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.fetch_add(10);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.fetch_add(22);

    let metric3 = family
        .register(&[("metric", "3"), ("label2", "baz")])
        .expect("metric 2 must register");
    metric3.fetch_add(33);

    assert_eq!(family.max_value(), Some(33));
}

#[test]
fn counter_total() {
    let family = {
        let builder = MetricBuilder::new("test_counter");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Counter, 4>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.fetch_add(10);

    // register an unrecorded metric to ensure that it doesn't get added to the
    // total.
    let _metric2 = family
        .register(&[("metric", "unrecorded"), ("label2", "lol")])
        .expect("metric 2 must register");

    let metric3 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric3.fetch_add(22);

    let metric4 = family
        .register(&[("metric", "3"), ("label2", "baz")])
        .expect("metric 2 must register");
    metric4.fetch_add(33);

    assert_eq!(family.total(), 65);
}

#[test]
fn counter_mean() {
    let family = {
        let builder = MetricBuilder::new("test_counter");
        #[cfg(feature = "timestamp")]
        let builder = builder.without_timestamps();
        builder.build::<Counter, 4>()
    };
    let metric1 = family
        .register(&[("metric", "1"), ("label2", "foo")])
        .expect("metric 1 must register");
    metric1.fetch_add(2);

    let metric2 = family
        .register(&[("metric", "2"), ("label2", "bar")])
        .expect("metric 2 must register");
    metric2.fetch_add(5);

    let metric3 = family
        .register(&[("metric", "3"), ("label2", "baz")])
        .expect("metric 2 must register");
    metric3.fetch_add(14);
    assert_eq!(family.mean(), Some(7));

    // N.B.: counters have different behavior with unrecorded values, since they
    // start at 0 as soon as they're registered.
    //
    // Registering a new counter without recording a value will set the mean to
    // 5, since there are now 4 counters rather than three.
    let _metric4 = family
        .register(&[("metric", "4"), ("label2", "qux")])
        .expect("metric 4 must register");
    assert_eq!(family.mean(), Some(5));
}
