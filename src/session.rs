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
        "readgp" => {
            let regs = tracee.read_general_purpose_registers();
            dbg!(regs.regs);
            dbg!(regs.sp);
            dbg!(regs.pc);
            dbg!(regs.pstate);
        }
        "writegp" => {
            let mut regs = tracee.read_general_purpose_registers();
            regs.sp = 99999999;
            tracee.write_general_purpose_registers(&mut regs);
        }
        "readfp" => {
            let regs = tracee.read_floating_point_registers();
            dbg!(regs.vregs);
            dbg!(regs.fpsr);
            dbg!(regs.fpcr);
        }
        "writefp" => {
            let mut regs = tracee.read_floating_point_registers();
            regs.fpcr = 99999999;
            tracee.write_floating_point_registers(&mut regs);
        }
        line => {
            println!("unexpected command: \"{}\"", line);
        }
    }
}
