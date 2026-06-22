use lazyifconfig::collector::system::parse_ps_process_metrics;

#[test]
fn ps_process_metrics_parse_cpu_percent_and_rss_memory() {
    let metrics = parse_ps_process_metrics("  4.2  131072\n").unwrap();

    assert_eq!(metrics.cpu_usage_tenths, Some(42));
    assert_eq!(metrics.memory_rss_bytes, Some(131_072 * 1024));
}

#[test]
fn ps_process_metrics_accept_command_column_prefix() {
    let metrics = parse_ps_process_metrics("lazyifconfig   0.7   65536\n").unwrap();

    assert_eq!(metrics.cpu_usage_tenths, Some(7));
    assert_eq!(metrics.memory_rss_bytes, Some(65_536 * 1024));
}

#[test]
fn windows_cpu_usage_divides_process_time_by_elapsed_time_and_cpu_count() {
    let cpu_tenths = lazyifconfig::collector::system::windows_cpu_usage_tenths(
        1_000_000, 5_000_000, 10_000_000, 4,
    );

    assert_eq!(cpu_tenths, Some(100));
}
