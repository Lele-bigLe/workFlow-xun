// Prevents additional console window on Windows directly
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

fn main() {
    xun_lib::run()
}
