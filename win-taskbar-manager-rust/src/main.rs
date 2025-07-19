mod taskbar;

use clap::Parser;
use env_logger::Env;
use log::error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use taskbar::{TaskbarEvent, TaskbarManager};
use tokio::time;

/// Windows Taskbar Manager - Hide taskbar and capture mouse events
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Top offset for workspace area (default: 0)
    #[arg(long, default_value_t = 0)]
    workspace_top_offset: i32,

    /// Bottom offset for workspace area (default: 0)
    #[arg(long, default_value_t = 0)]
    workspace_bottom_offset: i32,
}

/// Acil durum taskbar restore fonksiyonu
fn emergency_taskbar_restore() {
    // EXPLORER RESTART - En basit ve etkili çözüm
    let _ = std::process::Command::new("taskkill")
        .args(&["/F", "/IM", "explorer.exe"])
        .output();

    std::thread::sleep(std::time::Duration::from_millis(500));

    let _ = std::process::Command::new("explorer.exe").spawn();
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Command line argümanlarını parse et
    let args = Args::parse();

    // Logger'ı başlat
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    println!(
        "{{\"event_type\":\"startup\",\"workspace_top_offset\":{},\"workspace_bottom_offset\":{}}}",
        args.workspace_top_offset, args.workspace_bottom_offset
    );

    // TaskbarManager'ı offset'lerle oluştur
    let (mut taskbar_manager, mut event_receiver) =
        match TaskbarManager::new(args.workspace_top_offset, args.workspace_bottom_offset) {
            Ok((manager, receiver)) => (manager, receiver),
            Err(e) => {
                emergency_taskbar_restore();
                return Err(e);
            }
        };

    // Taskbar'ı gizle
    if let Err(e) = taskbar_manager.hide_taskbar() {
        emergency_taskbar_restore();
        return Err(e);
    }

    // Event listener task
    let _event_task = tokio::spawn(async move {
        while let Some(event) = event_receiver.recv().await {
            print_event_json(&event);
        }
    });

    // Guardian KALDIRILDI - Sadece başlangıçta gizle, sonra karışma
    println!("{{\"event_type\":\"guardian_disabled\",\"reason\":\"user_request\"}}");

    // Ctrl+C handler kurulumu
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let shutdown_flag_clone = shutdown_flag.clone();

    ctrlc::set_handler(move || {
        shutdown_flag_clone.store(true, Ordering::SeqCst);
    })?;

    // Ana event loop
    loop {
        let mut interval = time::interval(Duration::from_millis(100));

        tokio::select! {
            _ = interval.tick() => {
                if shutdown_flag.load(Ordering::SeqCst) {
                    break;
                }
                taskbar_manager.check_mouse_events();
            }
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }
    }

    // Program sonlanırken taskbar'ı geri göster
    match taskbar_manager.restore_taskbar() {
        Ok(_) => {}
        Err(_) => {
            emergency_taskbar_restore();
        }
    }

    Ok(())
}

fn print_event_json(event: &TaskbarEvent) {
    match serde_json::to_string(event) {
        Ok(json) => {
            println!("{}", json);
        }
        Err(e) => {
            error!("JSON serialization hatası: {}", e);
        }
    }
}
