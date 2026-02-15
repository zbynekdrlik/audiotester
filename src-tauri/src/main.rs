// Suppress console window on Windows (debugging via web UI and API)
#![windows_subsystem = "windows"]

fn main() {
    audiotester_app::run();
}
