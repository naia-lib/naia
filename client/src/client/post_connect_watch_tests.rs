use super::*;

#[test]
fn watch_starts_unfired_with_zeroed_counters() {
    let watch = PostConnectWatch::new();
    assert_eq!(watch.data_rx, 0);
    assert_eq!(watch.data_applied, 0);
    assert_eq!(watch.keepalive_sent, 0);
    assert_eq!(watch.last_keepalive_ok, None);
    assert!(!watch.first_data_warned);
    assert!(!watch.summary_30_warned);
    assert!(!watch.summary_60_warned);
}

#[test]
fn summaries_fire_once_at_30s_and_60s() {
    let mut watch = PostConnectWatch::new();
    assert_eq!(watch.summaries_due(29), (false, false));
    assert_eq!(watch.summaries_due(30), (true, false));
    assert_eq!(
        watch.summaries_due(31),
        (false, false),
        "the +30s summary must not repeat",
    );
    assert_eq!(watch.summaries_due(59), (false, false));
    assert_eq!(watch.summaries_due(60), (false, true));
    assert_eq!(
        watch.summaries_due(61),
        (false, false),
        "the +60s summary must not repeat",
    );
}
