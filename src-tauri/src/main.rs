// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    if std::env::args()
        .skip(1)
        .eq(["--purge-credentials-for-uninstall"])
    {
        std::process::exit(if ember_lib::purge_credentials_for_uninstall().is_ok() {
            0
        } else {
            1
        });
    }
    ember_lib::run()
}
