#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(feature = "gui")]
mod ui;

fn main() {
    #[cfg(feature = "gui")]
    ui::run();
    #[cfg(not(feature = "gui"))]
    eprintln!("Built without the window. Use --features gui.");
}
