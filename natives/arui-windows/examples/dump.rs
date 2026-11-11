//! Starts the tracker and prints every event as one JSON line for N seconds (default 20),
//! then a summary of the final list grouped by app id.
//!
//!   cargo run -p arui-windows --example dump -- [seconds] [--focus-test] [--save-icons <dir>]

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arui_win::{IconStore, NativeEvent};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let secs: u64 = args.first().and_then(|a| a.parse().ok()).unwrap_or(20);
    let t0 = Instant::now();
    let sink: arui_win::EventSink = Arc::new(move |ev: NativeEvent| {
        println!(
            "{:>7}ms {}",
            t0.elapsed().as_millis(),
            serde_json::to_string(&ev).unwrap()
        );
    });
    let icons = IconStore::new();
    let tracker = arui_windows::start(sink, icons.clone()).expect("start tracker");
    eprintln!("started in {}ms", t0.elapsed().as_millis());

    if args.iter().any(|a| a == "--focus-test") {
        // Toggle the first window twice to exercise minimize + focus.
        std::thread::sleep(Duration::from_secs(1));
        if let Some(w) = tracker.list().first() {
            eprintln!("toggle {} ({})", w.hwnd, w.title);
            eprintln!("  -> {:?}", tracker.toggle(w.hwnd));
            std::thread::sleep(Duration::from_secs(1));
            eprintln!("  -> {:?}", tracker.toggle(w.hwnd));
        }
    }

    std::thread::sleep(Duration::from_secs(secs));
    let list = tracker.list();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for w in &list {
        let icon = match icons.get(&w.icon_key) {
            Some(png) => format!("{} ({} bytes)", w.icon_key, png.len()),
            None => format!("{:?} (missing)", w.icon_key),
        };
        groups
            .entry(w.app_id.clone())
            .or_default()
            .push(format!("{:?} [{}] icon={}", w.title, w.class_name, icon));
    }
    eprintln!("\n{} windows in {} groups:", list.len(), groups.len());
    for (app, wins) in groups {
        eprintln!("  {app}");
        for w in wins {
            eprintln!("    {w}");
        }
    }
    if let Some(dir) = args
        .iter()
        .position(|a| a == "--save-icons")
        .and_then(|i| args.get(i + 1))
    {
        for w in &list {
            if let Some(png) = icons.get(&w.icon_key) {
                let name: String = w
                    .icon_key
                    .chars()
                    .map(|c| {
                        if c.is_ascii_alphanumeric() || c == '.' {
                            c
                        } else {
                            '_'
                        }
                    })
                    .collect();
                let name = &name[name.len().saturating_sub(60)..];
                let _ = std::fs::write(format!("{dir}/{name}.png"), png.as_slice());
            }
        }
    }
    drop(tracker);
    eprintln!("stopped cleanly");
}
