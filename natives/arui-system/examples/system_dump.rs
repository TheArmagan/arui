//! Prints system events as JSON for ~20 s.
//! `cargo run -p arui-system --example system_dump [secs] [nudge]`
//! With `nudge`, sets the volume 2 % lower and back again to exercise the setter + callback.
use std::sync::Arc;
use std::time::Duration;

fn main() {
    let secs: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let sink: arui_win::EventSink = Arc::new(|e| {
        println!("{}", serde_json::to_string(&e).unwrap());
    });
    let sys = arui_system::start(sink).expect("start system");
    eprintln!("volume() = {:?}", sys.volume());
    eprintln!("network() = {:?}", sys.network());
    if std::env::args().nth(2).as_deref() == Some("nudge") {
        let v = sys.volume();
        eprintln!("set_volume = {:?}", sys.set_volume(v.level - 0.02));
        std::thread::sleep(Duration::from_millis(300));
        eprintln!("set_volume back = {:?}", sys.set_volume(v.level));
        eprintln!("set_muted(same) = {:?}", sys.set_muted(v.muted));
    }
    std::thread::sleep(Duration::from_secs(secs));
    drop(sys);
    eprintln!("stopped cleanly");
}
