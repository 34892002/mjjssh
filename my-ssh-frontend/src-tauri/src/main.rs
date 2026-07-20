// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(linker_messages)] // MSVC reports normal import-library creation on stdout.

fn main() {
    app_lib::run();
}
