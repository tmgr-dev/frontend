// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
  tmgr_lib::maybe_run_mcp_bridge();
  tmgr_lib::run();
}
