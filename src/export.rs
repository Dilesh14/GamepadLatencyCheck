//! CSV export of retained app-observed interval samples.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::stats::{IntervalSample, WINDOW_CAPACITY};

pub struct ExportRequest<'a> {
    pub app_version: &'a str,
    pub controller_name: &'a str,
    pub controller_id: String,
    pub connected: bool,
    pub mapping: &'a str,
    pub power: &'a str,
    pub uuid: &'a str,
    pub samples: &'a [IntervalSample],
    pub exported_at: SystemTime,
    pub exported_instant: Instant,
}

/// Write the request as a CSV file in the user's Downloads directory and return
/// the path. Metadata is written as leading `#` comment lines so the sample table
/// stays machine-readable.
pub fn write_csv(request: &ExportRequest<'_>) -> Result<PathBuf, String> {
    let directory = export_directory();
    fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create {}: {error}", directory.display()))?;

    let exported_secs = unix_seconds(request.exported_at);
    let base = format!("gamepad-intervals-{}", format_utc_compact(exported_secs));
    let path = unique_path(&directory, &base);

    let contents = format_csv(request);
    let mut file = fs::File::create(&path)
        .map_err(|error| format!("could not create {}: {error}", path.display()))?;
    file.write_all(contents.as_bytes())
        .map_err(|error| format!("could not write {}: {error}", path.display()))?;

    Ok(path)
}

pub fn format_csv(request: &ExportRequest<'_>) -> String {
    let exported_secs = unix_seconds(request.exported_at);
    let mut out = String::new();

    out.push_str(&format!(
        "# Gamepad Latency Check {} - app-observed diagnostics export\n",
        sanitize(request.app_version)
    ));
    out.push_str(
        "# This file contains time between consecutive input events as received by this app.\n",
    );
    out.push_str(
        "# It is not controller polling rate and not physical button-to-screen latency.\n",
    );
    out.push_str(&format!(
        "# exported_at_utc={}\n",
        format_utc(exported_secs)
    ));
    out.push_str(&format!(
        "# platform={} {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    out.push_str(&format!("# interval_window_capacity={WINDOW_CAPACITY}\n"));
    out.push_str(&format!("# interval_count={}\n", request.samples.len()));
    out.push_str(&format!(
        "# controller_id={}\n",
        sanitize(&request.controller_id)
    ));
    out.push_str(&format!(
        "# controller_name={}\n",
        sanitize(request.controller_name)
    ));
    out.push_str(&format!(
        "# controller_connected_at_export={}\n",
        request.connected
    ));
    out.push_str(&format!(
        "# controller_mapping={}\n",
        sanitize(request.mapping)
    ));
    out.push_str(&format!("# controller_power={}\n", sanitize(request.power)));
    out.push_str(&format!("# controller_uuid={}\n", sanitize(request.uuid)));
    out.push_str("# sample_index,interval_ms,event_kind,observed_at_utc\n");

    for (index, sample) in request.samples.iter().enumerate() {
        let age = request
            .exported_instant
            .saturating_duration_since(sample.at);
        let observed_at = request
            .exported_at
            .checked_sub(age)
            .unwrap_or(request.exported_at);
        out.push_str(&format!(
            "{},{:.3},{},{}\n",
            index + 1,
            sample.interval.as_secs_f64() * 1000.0,
            sample.kind.as_str(),
            format_utc(unix_seconds(observed_at))
        ));
    }

    out
}

fn export_directory() -> PathBuf {
    if let Some(home) = home_directory() {
        let downloads = home.join("Downloads");
        if downloads.is_dir() {
            return downloads;
        }
        if home.is_dir() {
            return home;
        }
    }
    std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
}

fn home_directory() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

fn unique_path(directory: &Path, base: &str) -> PathBuf {
    let first = directory.join(format!("{base}.csv"));
    if !first.exists() {
        return first;
    }
    for suffix in 2..1000 {
        let candidate = directory.join(format!("{base}-{suffix}.csv"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{base}-{}.csv", unix_seconds(SystemTime::now())))
}

fn sanitize(value: &str) -> String {
    value
        .replace(['\n', '\r'], " ")
        .replace(',', ";")
        .trim()
        .to_owned()
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn format_utc(seconds: u64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(seconds);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

fn format_utc_compact(seconds: u64) -> String {
    let (year, month, day, hour, minute, second) = utc_parts(seconds);
    format!("{year:04}{month:02}{day:02}-{hour:02}{minute:02}{second:02}")
}

fn utc_parts(seconds: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = (seconds_of_day / 3_600) as u32;
    let minute = ((seconds_of_day % 3_600) / 60) as u32;
    let second = (seconds_of_day % 60) as u32;
    (year, month, day, hour, minute, second)
}

/// Howard Hinnant's days-from-civil algorithm, inverted.
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant, UNIX_EPOCH};

    use super::*;
    use crate::stats::EventKind;

    #[test]
    fn formats_known_unix_timestamp() {
        assert_eq!(format_utc(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(format_utc(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn csv_contains_metadata_and_samples() {
        let exported_at = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        let exported_instant = Instant::now();
        let samples = vec![
            IntervalSample {
                interval: Duration::from_micros(8_123),
                at: exported_instant - Duration::from_millis(100),
                kind: EventKind::Button,
            },
            IntervalSample {
                interval: Duration::from_micros(15_500),
                at: exported_instant - Duration::from_millis(50),
                kind: EventKind::Axis,
            },
        ];
        let request = ExportRequest {
            app_version: "0.1.0",
            controller_name: "Test, Pad",
            controller_id: "0".to_owned(),
            connected: true,
            mapping: "SDL gamepad mapping",
            power: "Wired / no battery",
            uuid: "00000000-0000-0000-0000-000000000000",
            samples: &samples,
            exported_at,
            exported_instant,
        };

        let csv = format_csv(&request);
        assert!(csv.contains("# controller_name=Test; Pad"));
        assert!(csv.contains("# interval_count=2"));
        assert!(csv.contains("1,8.123,button,2023-11-14T22:13:19Z"));
        assert!(csv.contains("2,15.500,axis,2023-11-14T22:13:19Z"));
        assert!(csv.ends_with('\n'));
    }
}
