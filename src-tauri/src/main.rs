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
    // It finds Tinto's server by itself; configs written before that also
    // pass a URL and token, kept for when Tinto has saved none.
    match args.as_slice() {
        [flag] if flag == tinto_lib::delivery::wiring::PROXY_FLAG => {
            std::process::exit(tinto_lib::delivery::wiring::proxy_stdio(None, None));
        }
        [flag, url, token] if flag == tinto_lib::delivery::wiring::PROXY_FLAG => {
            std::process::exit(tinto_lib::delivery::wiring::proxy_stdio(
                Some((url, token)),
                None,
            ));
        }
        // The Agents conversation it serves, to tell Tinto who is calling.
        [flag, url, token, session] if flag == tinto_lib::delivery::wiring::PROXY_FLAG => {
            std::process::exit(tinto_lib::delivery::wiring::proxy_stdio(
                Some((url, token)),
                Some(session),
            ));
        }
        _ => {}
    }
    tinto_lib::run()
}
