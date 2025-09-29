use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing_appender::rolling;
use tracing_subscriber::{filter::LevelFilter, fmt, prelude::*, util::SubscriberInitExt};

// Helper function to check for leap year
fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

// Custom implementation to format SystemTime as YYYYMMDDHHMMSS in local time (UTC+8)
fn get_formatted_timestamp(time: SystemTime) -> String {
    const SECS_IN_DAY: u64 = 86400;
    const UTC_OFFSET: u64 = 8 * 3600;

    let duration = time
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let total_secs = duration.as_secs() + UTC_OFFSET;

    let secs_of_day = total_secs % SECS_IN_DAY;
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;

    let mut days = total_secs / SECS_IN_DAY;
    let mut year = 1970;
    while days >= 365 {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }

    let months_len = if is_leap(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for &month_days in &months_len {
        if days < month_days {
            break;
        }
        days -= month_days;
        month += 1;
    }
    let day_of_month = days + 1;

    format!(
        "{:04}{:02}{:02}{:02}{:02}{:02}",
        year, month, day_of_month, hour, minute, second
    )
}

pub fn init_log() {
    // 1. Create logs directory
    fs::create_dir_all("logs").expect("Failed to create logs directory");

    // 2. Setup console layer for ERROR level
    let console_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(LevelFilter::ERROR);

    // 3. Setup file layer for INFO level
    let filename = format!("{}.log", get_formatted_timestamp(SystemTime::now()));
    let file_appender = rolling::never("logs", &filename);

    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false) // No colors in file
        .with_filter(LevelFilter::INFO);

    // 4. Combine layers and initialize
    tracing_subscriber::registry()
        .with(console_layer)
        .with(file_layer)
        .init();
}
