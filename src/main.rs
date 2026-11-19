//! nvwrites: how much has NVDisplay.Container.exe written to disk?
//! Usage: nvwrites [seconds-to-sample]   (default 10)
//! Windows only. No dependencies: raw kernel32 FFI.

use std::ffi::c_void;
use std::io::Read;
use std::{env, thread, time::Duration};

type Handle = *mut c_void;

#[repr(C)]
struct ProcessEntry32W {
    size: u32,
    usage: u32,
    pid: u32,
    heap_id: usize,
    module_id: u32,
    threads: u32,
    parent_pid: u32,
    pri_class_base: i32,
    flags: u32,
    exe_file: [u16; 260],
}

#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_ops: u64,
    write_ops: u64,
    other_ops: u64,
    read_bytes: u64,
    write_bytes: u64,
    other_bytes: u64,
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> Handle;
    fn Process32FirstW(snap: Handle, entry: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(snap: Handle, entry: *mut ProcessEntry32W) -> i32;
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
    fn GetProcessIoCounters(proc_: Handle, io: *mut IoCounters) -> i32;
    fn CloseHandle(h: Handle) -> i32;
    fn GetTickCount64() -> u64;
}

#[link(name = "shell32")]
extern "system" {
    fn ShellExecuteW(
        hwnd: Handle,
        op: *const u16,
        file: *const u16,
        params: *const u16,
        dir: *const u16,
        show: i32,
    ) -> isize;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Relaunch this exe with the UAC prompt. Returns false if the user said no.
fn relaunch_as_admin() -> bool {
    let Ok(exe) = env::current_exe() else { return false };
    let args: Vec<String> = env::args().skip(1).collect();
    let (op, file, params) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&args.join(" ")));
    // ShellExecuteW returns a value above 32 on success.
    unsafe { ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), file.as_ptr(), params.as_ptr(), std::ptr::null(), 1) > 32 }
}

const TH32CS_SNAPPROCESS: u32 = 0x2;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const TARGET: &str = "nvdisplay.container.exe";

/// PIDs of every running NVDisplay.Container.exe.
fn find_pids() -> Vec<u32> {
    let mut pids = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap as isize == -1 {
            return pids;
        }
        let mut e: ProcessEntry32W = std::mem::zeroed();
        e.size = std::mem::size_of::<ProcessEntry32W>() as u32;
        let mut ok = Process32FirstW(snap, &mut e);
        while ok != 0 {
            let len = e.exe_file.iter().position(|&c| c == 0).unwrap_or(260);
            if String::from_utf16_lossy(&e.exe_file[..len]).to_lowercase() == TARGET {
                pids.push(e.pid);
            }
            ok = Process32NextW(snap, &mut e);
        }
        CloseHandle(snap);
    }
    pids
}

/// Total bytes the process has written since it started, or None if access is denied.
fn bytes_written(pid: u32) -> Option<u64> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut io = IoCounters::default();
        let ok = GetProcessIoCounters(h, &mut io);
        CloseHandle(h);
        (ok != 0).then_some(io.write_bytes)
    }
}

fn fmt_bytes(b: f64) -> String {
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else {
        format!("{:.1} MB", b / MB)
    }
}

fn fmt_duration(secs: u64) -> String {
    let (h, m) = (secs / 3600, secs / 60 % 60);
    if h >= 24 {
        format!("{}d {}h {}m", h / 24, h % 24, m)
    } else {
        format!("{}h {}m", h, m)
    }
}

/// Healthy drivers write well under 1 MB/min; the buggy ones reach 100+ MB/min.
/// ponytail: fixed thresholds from public reports, tune if NVIDIA's numbers change.
fn verdict(mb_per_min: f64) -> &'static str {
    if mb_per_min < 2.0 {
        "OK - normal amount of writing"
    } else if mb_per_min < 20.0 {
        "ELEVATED - higher than normal; re-check after the PC has been on for an hour"
    } else {
        "AFFECTED - this looks like the NVIDIA driver write bug; roll back or update the driver"
    }
}

fn main() {
    let sample_secs: u64 = env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(10).max(1);
    let uptime_secs = unsafe { GetTickCount64() } / 1000;

    println!("nvwrites - NVDisplay.Container.exe disk writes\n");
    println!("PC uptime: {}", fmt_duration(uptime_secs));

    let pids = find_pids();
    if pids.is_empty() {
        println!("NVDisplay.Container.exe is not running. Nothing to check.");
        pause();
        return;
    }

    let start: Vec<(u32, Option<u64>)> = pids.iter().map(|&p| (p, bytes_written(p))).collect();
    let mut total = 0u64;
    for (pid, w) in &start {
        match w {
            Some(b) => {
                total += b;
                println!("  PID {pid:>6}: {} written since it started", fmt_bytes(*b as f64));
            }
            None => println!("  PID {pid:>6}: access denied (run as administrator)"),
        }
    }
    if start.iter().all(|(_, w)| w.is_none()) {
        println!("\nWindows needs admin rights to read the NVIDIA service. Asking now...");
        if relaunch_as_admin() {
            return; // the admin copy opens in its own window
        }
        println!("Admin was declined. Right-click nvwrites.exe -> Run as administrator.");
        pause();
        return;
    }
    println!("Total since boot (approx.): {}", fmt_bytes(total as f64));
    if uptime_secs > 0 {
        println!("Average: {}/min", fmt_bytes(total as f64 / (uptime_secs as f64 / 60.0)));
    }

    println!("\nMeasuring live rate for {sample_secs}s...");
    thread::sleep(Duration::from_secs(sample_secs));
    let delta: u64 = start
        .iter()
        .filter_map(|&(p, w0)| Some(bytes_written(p)?.saturating_sub(w0?)))
        .sum();
    let per_min = delta as f64 * 60.0 / sample_secs as f64;
    let mb_per_min = per_min / (1024.0 * 1024.0);
    println!("Live rate: {}/min (~{}/day if left on)", fmt_bytes(per_min), fmt_bytes(per_min * 1440.0));
    println!("\nVerdict: {}", verdict(mb_per_min));
    pause();
}

/// Keep the window open when double-clicked.
fn pause() {
    println!("\nPress Enter to exit.");
    let _ = std::io::stdin().read(&mut [0u8]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_verdicts() {
        assert_eq!(fmt_bytes(75.8 * 1024.0 * 1024.0), "75.8 MB");
        assert_eq!(fmt_bytes(222.0 * 1024.0 * 1024.0 * 1024.0), "222.00 GB");
        assert_eq!(fmt_duration(38 * 3600 + 30 * 60), "1d 14h 30m");
        assert!(verdict(0.4).starts_with("OK"));
        assert!(verdict(10.0).starts_with("ELEVATED"));
        assert!(verdict(305.0).starts_with("AFFECTED"));
    }

    #[test]
    fn struct_size_matches_win32() {
        // PROCESSENTRY32W is 568 bytes on x64.
        #[cfg(target_pointer_width = "64")]
        assert_eq!(std::mem::size_of::<ProcessEntry32W>(), 568);
    }
}
