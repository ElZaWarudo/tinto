// Keep the plugin ACL default empty. The application command is opt-in via
// `wdio-webdriver:allow-request-dialog` in a private test capability.
const COMMANDS: &[&str] = &["request_dialog"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .ios_path("ios")
        .build();
}
