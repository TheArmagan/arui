//! `arui-guard.exe --pid <PID> [--state-file <path>]`
//!
//! Waits for the ARUI host process to exit. If the host left its arui-shell state file behind
//! (it deletes it on a clean shutdown), the host died abnormally: remove its AppBars, restore
//! explorer's auto-hide setting and show the taskbars again.

// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use arui_shell::{emergency_restore_state, ShellState};
use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Threading::{OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE};

struct Args {
    pid: u32,
    state_file: PathBuf,
}

fn parse_args(mut it: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut pid = None;
    let mut state_file = None;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--pid" => pid = Some(it.next().ok_or("--pid needs a value")?.parse::<u32>().map_err(|e| e.to_string())?),
            "--state-file" => state_file = Some(PathBuf::from(it.next().ok_or("--state-file needs a value")?)),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(Args {
        pid: pid.ok_or("missing --pid")?,
        state_file: state_file.unwrap_or_else(arui_shell::default_state_file),
    })
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("arui-guard: {e}\nusage: arui-guard --pid <PID> [--state-file <path>]");
            std::process::exit(2);
        }
    };
    unsafe {
        // If the process is already gone OpenProcess fails; go straight to the restore check.
        if let Ok(h) = OpenProcess(PROCESS_SYNCHRONIZE, false, args.pid) {
            WaitForSingleObject(h, INFINITE);
            let _ = CloseHandle(h);
        }
    }
    match ShellState::load(&args.state_file) {
        Some(st) if st.pid == args.pid => {
            eprintln!("arui-guard: pid {} exited without cleanup, restoring", args.pid);
            emergency_restore_state(&st);
            ShellState::delete(&args.state_file);
        }
        Some(st) => eprintln!("arui-guard: state file belongs to pid {}, leaving it", st.pid),
        None => eprintln!("arui-guard: clean exit"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Result<Args, String> {
        parse_args(v.iter().map(|s| s.to_string()))
    }

    #[test]
    fn parses() {
        let a = args(&["--pid", "42", "--state-file", "x.json"]).unwrap();
        assert_eq!(a.pid, 42);
        assert_eq!(a.state_file, PathBuf::from("x.json"));
        assert_eq!(args(&["--pid", "7"]).unwrap().state_file, arui_shell::default_state_file());
        assert!(args(&[]).is_err());
        assert!(args(&["--pid", "x"]).is_err());
        assert!(args(&["--bogus"]).is_err());
    }
}
