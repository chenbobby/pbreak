use std::io::{stdin, stdout, BufRead, Write};

use crate::tracee::Tracee;

pub unsafe fn run_session(tracee: &mut Tracee) {
    let stdin = stdin();
    let mut stdout = stdout();

    write!(stdout, "pbreak> ").unwrap();
    stdout.flush().unwrap();

    for line_result in stdin.lock().lines() {
        match line_result {
            Err(err) => {
                println!("failed to read line from stdin: {}", err);
            }
            Ok(line) => handle_command(tracee, &line),
        }

        write!(stdout, "pbreak> ").unwrap();
        stdout.flush().unwrap();
    }
}

pub unsafe fn handle_command(tracee: &mut Tracee, line: &str) {
    match line {
        "continue" => {
            tracee.resume();
            tracee.wait_on_signal();
        }
        line => {
            println!("unexpected command: \"{}\"", line);
        }
    }
}
