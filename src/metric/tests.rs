use super::*;
use pretty_assertions::assert_str_eq;

#[test]
fn gauge() {
    let family = MetricBuilder::new("test_gauge")
        .with_help("a test gauge")
        .with_unit("tests")
        .build::<Gauge, 2>();
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
    # HELP test_gauge a test gauge\n\
    # UNIT test_gauge tests\n\
    test_gauge{metric=\"1\",label2=\"foo\"} 10\n\
    test_gauge{metric=\"2\",label2=\"bar\"} 22.2\n\n\
    ";
    assert_str_eq!(family.to_string(), expected);
}

#[test]
fn counter() {
    let family = MetricBuilder::new("test_counter")
        .with_help("a test counter")
        .with_unit("tests")
        .build::<Counter, 2>();
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
    # HELP test_counter a test counter\n\
    # UNIT test_counter tests\n\
    test_counter{metric=\"1\",label2=\"foo\"} 1\n\
    test_counter{metric=\"2\",label2=\"bar\"} 2\n\n\
    ";
    assert_str_eq!(family.to_string(), expected);
}
