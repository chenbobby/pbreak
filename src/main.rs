use std::process::exit;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = pbreak::cli::Command::from_args(&args);
    exit(unsafe { command.run() });
}
