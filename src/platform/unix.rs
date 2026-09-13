//! What the macOS and Linux backends share: the local date and the socket
//! that carries a command to the resident.

use crate::dbg_log;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

pub fn today() -> (i32, u32, u32) {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    unsafe { libc::localtime_r(&now, &mut tm) };
    (tm.tm_year + 1900, tm.tm_mon as u32 + 1, tm.tm_mday as u32)
}

/// Sends one command to the resident. false when no resident answers.
pub fn send(sock: &Path, cmd: &str) -> bool {
    UnixStream::connect(sock).and_then(|mut s| s.write_all(cmd.as_bytes())).is_ok()
}

/// Opens the resident's socket and reads commands from it until the process ends. Holding the
/// socket is what makes the instance unique: false means another resident answers on it.
pub fn listen(sock: PathBuf, on_command: impl Fn(&str) + Send + 'static) -> bool {
    let listener = match UnixListener::bind(&sock) {
        Ok(l) => l,
        Err(_) => {
            if UnixStream::connect(&sock).is_ok() {
                return false;
            }
            // socket left behind by a dead resident
            let _ = std::fs::remove_file(&sock);
            match UnixListener::bind(&sock) {
                Ok(l) => l,
                Err(e) => {
                    dbg_log(&format!("socket {}: {e}", sock.display()));
                    return false;
                }
            }
        }
    };
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut line = String::new();
            if BufReader::new(stream).read_line(&mut line).is_err() {
                continue;
            }
            let cmd = line.trim();
            if cmd.is_empty() {
                continue; // the single-instance probe: it connects without writing anything
            }
            dbg_log(&format!("socket: {cmd}"));
            on_command(cmd);
        }
    });
    true
}
