// Prevents an additional console window on Windows in release builds (no-op on Linux).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    nettune_lib::run()
}
