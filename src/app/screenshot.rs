use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use bevy::prelude::*;
use bevy::render::view::window::screenshot::Screenshot;
use bevy::render::view::window::screenshot::save_to_disk;

use crate::app::state::AppScreen;

/// Directory, relative to the working directory, where screenshots are written.
pub const SCREENSHOT_DIR: &str = "screenshots";

pub struct ScreenshotPlugin;

impl Plugin for ScreenshotPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            capture_screenshot.run_if(in_state(AppScreen::Playing)),
        );
    }
}

fn capture_screenshot(keys: Res<ButtonInput<KeyCode>>, mut commands: Commands) {
    if !keys.just_pressed(KeyCode::F2) {
        return;
    }

    let path = screenshot_path(SCREENSHOT_DIR, SystemTime::now());
    if let Some(parent) = path.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        error!(
            "Cannot create screenshot directory {}: {error}",
            parent.display()
        );
        return;
    }

    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(path));
}

/// Builds the file path for a screenshot taken at `now`, for example
/// `screenshots/2026-09-20_14-05-09.png`.
pub fn screenshot_path(dir: impl AsRef<Path>, now: SystemTime) -> PathBuf {
    let seconds = now
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    dir.as_ref().join(format!("{}.png", timestamp(seconds)))
}

/// Formats Unix seconds as a UTC `YYYY-MM-DD_HH-MM-SS` timestamp.
pub fn timestamp(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let seconds_of_day = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (
        seconds_of_day / 3_600,
        seconds_of_day / 60 % 60,
        seconds_of_day % 60,
    );
    format!("{year:04}-{month:02}-{day:02}_{hour:02}-{minute:02}-{second:02}")
}

/// Converts days since 1970-01-01 into a civil `(year, month, day)`.
///
/// Based on Howard Hinnant's `civil_from_days` algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = (shifted - era * 146_097) as u64;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}
