use crate::{session::run_session, tracee::Tracee};
use std::num::ParseIntError;

pub enum Command {
    Missing,
    Attach { pid: libc::pid_t },
    Fork { program: String, args: Vec<String> },
}

impl Command {
    // Constructs a `Command` from command line arguments.
    pub fn from_args(args: &[String]) -> Command {
        if args.len() == 1 {
            return Command::Missing;
        }

        if args.len() == 3 && args[1] == "-p" {
            let pid_str = args[2].as_str();
            let pid = match pid_str.parse::<libc::c_int>() {
                Err(ParseIntError { .. }) => {
                    panic!("invalid value for -p: \"{}\"", pid_str);
                }
                Ok(pid) => pid,
            };

            return Command::Attach { pid: pid };
        }

        return Command::Fork {
            program: args[1].to_string(),
            args: args.iter().skip(2).map(|s| s.clone()).collect(),
        };
    }

    // Executes the command.
    pub unsafe fn run(&self) -> i32 {
        return match self {
            Command::Missing => self.run_missing(),
            Command::Attach { pid } => self.run_attach(*pid),
            Command::Fork { program, args } => self.run_fork(program, args),
        };
    }

    fn run_missing(&self) -> i32 {
        println!("Missing command.");
        return -1;
    }

    unsafe fn run_attach(&self, pid: libc::pid_t) -> ! {
        let mut tracee = Tracee::from_pid(pid);
        run_session(&mut tracee);
        unreachable!("session should not terminate without exiting");
    }

    unsafe fn run_fork(&self, program: &str, args: &[String]) -> ! {
        let mut tracee = Tracee::from_cmd(program, args);
        run_session(&mut tracee);
        unreachable!("session should not terminate without exiting");
    }
}
