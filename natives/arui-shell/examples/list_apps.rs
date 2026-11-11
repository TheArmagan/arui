//! Read-only: starts the shell without touching the taskbar and prints the start menu index.
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use arui_win::{IconStore, NativeEvent, ShellEvent};

fn main() {
    let (tx, rx) = mpsc::channel();
    let tx = Mutex::new(tx);
    let icons = IconStore::new();
    let t = Instant::now();
    let shell = arui_shell::start(
        Arc::new(move |e| {
            if let NativeEvent::Shell(ShellEvent::AppsChanged(apps)) = e {
                let _ = tx.lock().unwrap().send(apps);
            }
        }),
        icons.clone(),
        arui_shell::ShellOptions::default(),
    )
    .expect("start");
    for m in shell.monitors() {
        println!("{m:?}");
    }
    let apps = rx.recv_timeout(Duration::from_secs(60)).expect("apps");
    println!("{} apps in {:?}", apps.len(), t.elapsed());
    let with_icon = apps.iter().filter(|a| icons.contains(&a.icon_key)).count();
    println!("{with_icon} with icons");
    for a in apps.iter().take(400) {
        println!("{:?} | {} | {:?} | {:?} | {:?}", a.kind, a.name, a.folder, a.app_id, a.exe_path);
    }
    if let Some(a) = apps.iter().find(|a| icons.contains(&a.icon_key)) {
        let png = icons.get(&a.icon_key).unwrap();
        let out = std::env::temp_dir().join("arui-icon-sample.png");
        std::fs::write(&out, png.as_slice()).unwrap();
        println!("sample icon for {} -> {}", a.name, out.display());
    }
}
