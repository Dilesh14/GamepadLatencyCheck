use std::time::Duration;

pub fn session_csv(
    device_name: &str,
    session_duration: Duration,
    interval_samples_ms: impl IntoIterator<Item = f64>,
) -> String {
    let mut csv = String::from("record_type,field,value,sample_index,interval_ms\n");
    write_metadata(&mut csv, "app_version", env!("CARGO_PKG_VERSION"));
    write_metadata(
        &mut csv,
        "measurement_scope",
        "software-observed event processing intervals; not physical latency",
    );
    write_metadata(&mut csv, "device_name", device_name);
    write_metadata(
        &mut csv,
        "session_duration_seconds",
        &format!("{:.3}", session_duration.as_secs_f64()),
    );

    for (index, sample_ms) in interval_samples_ms.into_iter().enumerate() {
        csv.push_str(&format!("interval_sample,,,{index},{sample_ms:.6}\n"));
    }

    csv
}

fn write_metadata(csv: &mut String, field: &str, value: &str) {
    csv.push_str("metadata,");
    csv.push_str(&escape_csv(field));
    csv.push(',');
    csv.push_str(&escape_csv(value));
    csv.push_str(",,\n");
}

fn escape_csv(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::session_csv;
    use std::time::Duration;

    #[test]
    fn exports_metadata_and_interval_samples_as_csv() {
        let csv = session_csv("Pad, \"Pro\"", Duration::from_millis(1_234), [4.25, 8.5]);

        assert!(csv.starts_with("record_type,field,value,sample_index,interval_ms\n"));
        assert!(csv.contains("metadata,device_name,\"Pad, \"\"Pro\"\"\",,\n"));
        assert!(csv.contains("metadata,session_duration_seconds,1.234,,\n"));
        assert!(csv.contains("interval_sample,,,0,4.250000\n"));
        assert!(csv.contains("interval_sample,,,1,8.500000\n"));
    }
}
