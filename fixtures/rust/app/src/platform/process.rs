use std::net::TcpListener;
use std::process::Command;

pub fn spawn_worker(port: u16) -> bool {
    let listening = TcpListener::bind(("127.0.0.1", port)).is_ok();
    let handle = std::thread::spawn(move || Command::new("true").status().is_ok());
    listening && handle.join().unwrap_or(false)
}
