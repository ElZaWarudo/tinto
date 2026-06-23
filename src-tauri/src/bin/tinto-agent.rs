use std::io::BufRead;

use tinto_lib::wsl_agent::runtime::respond_to_request_line;

fn main() {
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    match read {
        Ok(0) => {
            eprintln!("child_exit: stdin closed before handshake");
            std::process::exit(2);
        }
        Ok(_) => match respond_to_request_line(&line) {
            Ok(response) => {
                print!("{response}");
            }
            Err(error) => {
                eprintln!("{}: {}", error.safe_category(), error.message);
                std::process::exit(2);
            }
        },
        Err(_) => {
            eprintln!("child_exit: failed to read handshake");
            std::process::exit(2);
        }
    }
}
