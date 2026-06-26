use tinto_lib::wsl_agent::runtime::serve_request_lines;

fn main() {
    if let Err(error) = serve_request_lines(std::io::stdin().lock(), std::io::stdout().lock()) {
        eprintln!("{}: {}", error.safe_category(), error.message);
        std::process::exit(2);
    }
}
