#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
fn main() -> Result<(), String> {
    cal_based::widgets::run()
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("cal-based currently has only a Windows host");
}
