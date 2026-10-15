// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Agents in WSL reach Delivery's coordinator API through this mode.
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Claude Code's hooks in Agents report its turns through this mode.
    if let [flag, path] = args.as_slice() {
        if flag == tinto_lib::agent_console::claude_hooks::AGENT_HOOK_FLAG {
            std::process::exit(tinto_lib::agent_console::claude_hooks::append_event(path));
        }
    }
    if let [flag, url, token] = args.as_slice() {
        if flag == tinto_lib::delivery::wiring::PROXY_FLAG {
            std::process::exit(tinto_lib::delivery::wiring::proxy_stdio(url, token));
        }
    }
    tinto_lib::run()
}
