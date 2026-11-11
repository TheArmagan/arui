//! Pure helpers (no WinRT), unit tested.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;

use arui_win::{MediaSession, PlaybackStatus};

/// 100 ns ticks between 1601-01-01 (WinRT DateTime epoch) and 1970-01-01.
const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;

/// Maps the raw `GlobalSystemMediaTransportControlsSessionPlaybackStatus` value.
pub fn map_status(raw: i32) -> PlaybackStatus {
    match raw {
        0 => PlaybackStatus::Closed,
        1 => PlaybackStatus::Opened,
        2 => PlaybackStatus::Changing,
        3 => PlaybackStatus::Stopped,
        4 => PlaybackStatus::Playing,
        5 => PlaybackStatus::Paused,
        _ => PlaybackStatus::Closed,
    }
}

/// TimeSpan ticks (100 ns) to milliseconds, negative clamped to 0.
pub fn ticks_to_ms(ticks: i64) -> u64 {
    (ticks.max(0) / 10_000) as u64
}

/// WinRT DateTime (100 ns since 1601) to unix ms. `None` for zero / pre-1970 values, which
/// is what sessions report when they never set a timeline.
pub fn datetime_to_unix_ms(universal_time: i64) -> Option<u64> {
    if universal_time <= EPOCH_DIFF_TICKS {
        None
    } else {
        Some(((universal_time - EPOCH_DIFF_TICKS) / 10_000) as u64)
    }
}

/// Derives (position_ms, duration_ms) from timeline ticks. Duration is `end - start`; a
/// zero/negative duration means the app publishes no timeline. Position is relative to start.
pub fn timeline_ms(start: i64, end: i64, position: i64) -> (Option<u64>, Option<u64>) {
    let duration = if end > start { Some(ticks_to_ms(end - start)) } else { None };
    let position = if duration.is_none() && position <= 0 {
        None
    } else {
        Some(ticks_to_ms(position - start))
    };
    (position, duration)
}

/// Picks a unique session id: `base` if free, otherwise `base#2`, `base#3`, ...
pub fn assign_id<'a>(base: &str, taken: impl Iterator<Item = &'a str> + Clone) -> String {
    let base = if base.is_empty() { "unknown" } else { base };
    let is_taken = |c: &str| taken.clone().any(|t| t == c);
    if !is_taken(base) {
        return base.to_string();
    }
    (2..)
        .map(|n| format!("{base}#{n}"))
        .find(|c| !is_taken(c))
        .unwrap()
}

/// Position drift (ms) below which a new timeline sample counts as "the same".
const POSITION_TOLERANCE_MS: i64 = 1000;

/// True if `new` carries no information the UI does not already have from `old`: every field
/// equal except the timeline sample, and the new sample is what the UI would extrapolate
/// anyway (same position while not playing, or old position + elapsed time while playing).
/// Apps like Spotify re-publish an identical timeline every few seconds.
pub fn is_redundant(old: &MediaSession, new: &MediaSession) -> bool {
    let strip = |s: &MediaSession| MediaSession {
        position_ms: None,
        position_at_ms: None,
        ..s.clone()
    };
    if strip(old) != strip(new) {
        return false;
    }
    match (old.position_ms, new.position_ms, old.position_at_ms, new.position_at_ms) {
        (None, None, _, _) => true,
        (Some(op), Some(np), Some(oa), Some(na)) => {
            let expected = if new.status == PlaybackStatus::Playing {
                op as i64 + (na as i64 - oa as i64)
            } else {
                op as i64
            };
            (np as i64 - expected).abs() < POSITION_TOLERANCE_MS
        }
        _ => false,
    }
}

pub fn hash_bytes(b: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Returns PNG bytes: passes PNG through untouched, decodes anything else the `image` crate
/// understands (JPEG) and re-encodes it as PNG. `None` if the data is not a supported image.
pub fn ensure_png(bytes: Vec<u8>) -> Option<Vec<u8>> {
    if bytes.starts_with(PNG_MAGIC) {
        return Some(bytes);
    }
    let img = image::load_from_memory(&bytes).ok()?;
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).ok()?;
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping() {
        assert_eq!(map_status(4), PlaybackStatus::Playing);
        assert_eq!(map_status(5), PlaybackStatus::Paused);
        assert_eq!(map_status(3), PlaybackStatus::Stopped);
        assert_eq!(map_status(2), PlaybackStatus::Changing);
        assert_eq!(map_status(1), PlaybackStatus::Opened);
        assert_eq!(map_status(0), PlaybackStatus::Closed);
        assert_eq!(map_status(99), PlaybackStatus::Closed);
    }

    #[test]
    fn timespans() {
        assert_eq!(ticks_to_ms(10_000), 1);
        assert_eq!(ticks_to_ms(-5), 0);
        // 3 min track, 1 min in
        let (p, d) = timeline_ms(0, 1_800_000_000, 600_000_000);
        assert_eq!((p, d), (Some(60_000), Some(180_000)));
        // start offset
        let (p, d) = timeline_ms(100_000_000, 200_000_000, 150_000_000);
        assert_eq!((p, d), (Some(5_000), Some(10_000)));
        // no timeline at all
        assert_eq!(timeline_ms(0, 0, 0), (None, None));
        // position but no duration (live stream)
        assert_eq!(timeline_ms(0, 0, 20_000), (Some(2), None));
    }

    #[test]
    fn datetime_conversion() {
        assert_eq!(datetime_to_unix_ms(0), None);
        assert_eq!(datetime_to_unix_ms(EPOCH_DIFF_TICKS), None);
        assert_eq!(datetime_to_unix_ms(EPOCH_DIFF_TICKS + 10_000), Some(1));
        // 2024-01-01T00:00:00Z = 1704067200000 ms
        assert_eq!(
            datetime_to_unix_ms(EPOCH_DIFF_TICKS + 1_704_067_200_000 * 10_000),
            Some(1_704_067_200_000)
        );
    }

    fn sample(status: PlaybackStatus, pos: u64, at: u64) -> MediaSession {
        MediaSession {
            id: "a".into(),
            app_id: "a".into(),
            title: "t".into(),
            artist: String::new(),
            album: String::new(),
            status,
            can_play: true,
            can_pause: true,
            can_next: true,
            can_previous: true,
            position_ms: Some(pos),
            duration_ms: Some(200_000),
            position_at_ms: Some(at),
            artwork_key: None,
        }
    }

    #[test]
    fn redundancy() {
        use PlaybackStatus::*;
        // Paused, same position re-published later: redundant.
        assert!(is_redundant(&sample(Paused, 5_000, 1_000), &sample(Paused, 5_000, 9_000)));
        // Paused but seeked: not redundant.
        assert!(!is_redundant(&sample(Paused, 5_000, 1_000), &sample(Paused, 60_000, 9_000)));
        // Playing, position advanced by elapsed time: redundant.
        assert!(is_redundant(&sample(Playing, 5_000, 1_000), &sample(Playing, 13_100, 9_000)));
        // Playing, seek: not redundant.
        assert!(!is_redundant(&sample(Playing, 5_000, 1_000), &sample(Playing, 100_000, 9_000)));
        // Status change: never redundant.
        assert!(!is_redundant(&sample(Playing, 5_000, 1_000), &sample(Paused, 5_000, 1_000)));
        // Title change: never redundant.
        let mut n = sample(Paused, 5_000, 1_000);
        n.title = "other".into();
        assert!(!is_redundant(&sample(Paused, 5_000, 1_000), &n));
    }

    #[test]
    fn id_disambiguation() {
        let none: Vec<&str> = vec![];
        assert_eq!(assign_id("Spotify.exe", none.iter().copied()), "Spotify.exe");
        let t = ["Spotify.exe"];
        assert_eq!(assign_id("Spotify.exe", t.iter().copied()), "Spotify.exe#2");
        let t = ["Chrome", "Chrome#2"];
        assert_eq!(assign_id("Chrome", t.iter().copied()), "Chrome#3");
        let t = ["Chrome", "Chrome#3"];
        assert_eq!(assign_id("Chrome", t.iter().copied()), "Chrome#2");
        assert_eq!(assign_id("", none.iter().copied()), "unknown");
    }

    #[test]
    fn png_passthrough_and_jpeg_conversion() {
        // PNG passes through byte-identical.
        let png = arui_win::icons::rgba_to_png(1, 1, &[1, 2, 3, 255]).unwrap();
        assert_eq!(ensure_png(png.clone()).unwrap(), png);

        // JPEG is converted.
        let img = image::RgbImage::from_pixel(4, 4, image::Rgb([200, 10, 10]));
        let mut jpeg = Vec::new();
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut Cursor::new(&mut jpeg), image::ImageFormat::Jpeg)
            .unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
        let out = ensure_png(jpeg).unwrap();
        assert!(out.starts_with(PNG_MAGIC));

        // Garbage is rejected.
        assert!(ensure_png(vec![1, 2, 3]).is_none());
    }
}
