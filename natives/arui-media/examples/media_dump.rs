//! Prints media events as JSON for ~20 s.
//! `cargo run -p arui-media --example media_dump [secs] [pause]`
//! With `pause`, also sends Pause to the current session (harmless if already paused).
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let icons = arui_win::IconStore::new();
    let sink: arui_win::EventSink = Arc::new(|e| {
        println!("{}", serde_json::to_string(&e).unwrap());
    });
    let media = arui_media::start(sink, icons.clone()).expect("start media");
    let (sessions, current) = media.snapshot();
    eprintln!("snapshot(): {} session(s), current = {current:?}", sessions.len());
    for s in &sessions {
        if let Some(k) = &s.artwork_key {
            let len = icons.get(k).map(|b| b.len()).unwrap_or(0);
            eprintln!("  {} -> artwork {k} ({len} bytes PNG)", s.id);
        }
    }
    if std::env::args().nth(2).as_deref() == Some("pause") {
        eprintln!("control(None, Pause) = {:?}", media.control(None, arui_win::MediaAction::Pause));
    }
    eprintln!("control(bogus) = {:?}", media.control(Some("bogus"), arui_win::MediaAction::Play));
    std::thread::sleep(Duration::from_secs(secs));
    drop(media);
    eprintln!("stopped cleanly");
}
