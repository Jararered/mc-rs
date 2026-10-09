use std::time::Duration;
use std::time::UNIX_EPOCH;

use game::app::screenshot::SCREENSHOT_DIR;
use game::app::screenshot::numbered_path;
use game::app::screenshot::screenshot_path;
use game::app::screenshot::timestamp;

#[test]
fn timestamp_formats_known_utc_instants() {
    assert_eq!(timestamp(0), "1970-01-01_00-00-00");
    assert_eq!(timestamp(1_000_000_000), "2001-09-09_01-46-40");
    assert_eq!(timestamp(1_700_000_000), "2023-11-14_22-13-20");
    // Leap day.
    assert_eq!(timestamp(1_709_164_800), "2024-02-29_00-00-00");
}

#[test]
fn screenshot_path_uses_directory_and_png_extension() {
    let now = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    let path = screenshot_path(SCREENSHOT_DIR, now);
    assert_eq!(
        path,
        std::path::Path::new("screenshots").join("2001-09-09_01-46-40.png")
    );
}

#[test]
fn screenshots_in_the_same_second_get_numbered_names() {
    let path = std::path::Path::new("screenshots").join("2001-09-09_01-46-40.png");
    assert_eq!(numbered_path(&path, 0), path);
    assert_eq!(
        numbered_path(&path, 1),
        std::path::Path::new("screenshots").join("2001-09-09_01-46-40_1.png")
    );
    assert_eq!(
        numbered_path(&path, 12),
        std::path::Path::new("screenshots").join("2001-09-09_01-46-40_12.png")
    );
}
