pub mod ipc {
    use std::ffi::CString;

    pub struct Pipe {
        read_fd: libc::c_int,
        write_fd: libc::c_int,
    }

    impl Pipe {
        pub unsafe fn new() -> Pipe {
            let mut fds = vec![0; 2];
            if libc::pipe2(fds.as_mut_ptr(), 0) < 0 {
                let errno_message = CString::from_raw(libc::strerror(*libc::__errno_location()))
                    .into_string()
                    .unwrap();
                panic!("failed to open pipe: {}", errno_message);
            };

            return Pipe {
                read_fd: fds[0],
                write_fd: fds[1],
            };
        }

        // Reads a string out of the pipe.
        pub unsafe fn receive(&self) -> String {
            let mut s = String::with_capacity(128);
            if libc::read(
                self.read_fd,
                s.as_mut_ptr() as *mut libc::c_void,
                s.capacity(),
            ) < 0
            {
                let errno_message = CString::from_raw(libc::strerror(*libc::__errno_location()))
                    .into_string()
                    .unwrap();
                panic!(
                    "failed to read from pipe fd ({}): {}",
                    self.read_fd, errno_message,
                );
            }
            return s;
        }

        // Sends a string into a pipe.
        pub unsafe fn send(&self, s: &str) {
            if libc::write(self.write_fd, s.as_ptr() as *const libc::c_void, s.len()) < 0 {
                let errno_message = CString::from_raw(libc::strerror(*libc::__errno_location()))
                    .into_string()
                    .unwrap();
                panic!(
                    "failed to write into pipe fd ({}): {}",
                    self.write_fd, errno_message
                );
            }
        }

        // Closes the receiving end of the pipe.
        pub unsafe fn close_receiver(&mut self) {
            if self.read_fd != -1 {
                if libc::close(self.read_fd) < 0 {
                    let errno_message =
                        CString::from_raw(libc::strerror(*libc::__errno_location()))
                            .into_string()
                            .unwrap();
                    panic!(
                        "failed to close pipe's read file descriptor: {}",
                        errno_message,
                    );
                };

                self.read_fd = -1;
            }
        }

        // Close the sending end of the pipe.
        pub unsafe fn close_sender(&mut self) {
            if self.write_fd != -1 {
                if libc::close(self.write_fd) < 0 {
                    let errno_message =
                        CString::from_raw(libc::strerror(*libc::__errno_location()))
                            .into_string()
                            .unwrap();
                    panic!(
                        "failed to close pipe's write file descriptor: {}",
                        errno_message,
                    );
                };

                self.write_fd = -1;
            }
        }
    }

    impl Drop for Pipe {
        fn drop(&mut self) {
            unsafe {
                self.close_receiver();
                self.close_sender();
            }
        }
    }
}

pub mod session {
    use std::ffi::CString;
    use std::io::{stdin, stdout, BufRead, Write};
    use std::ptr::null_mut;

    pub unsafe fn run_session(pid: i32) {
        let stdin = stdin();
        let mut stdout = stdout();

        write!(stdout, "pbreak> ").unwrap();
        stdout.flush().unwrap();

        for line_result in stdin.lock().lines() {
            match line_result {
                Err(err) => {
                    println!("failed to read line from stdin: {}", err);
                }
                Ok(line) => handle_command(pid, &line),
            }

            write!(stdout, "pbreak> ").unwrap();
            stdout.flush().unwrap();
        }
    }

    pub unsafe fn handle_command(pid: i32, line: &str) {
        match line {
            "continue" => {
                if libc::ptrace(
                    libc::PTRACE_CONT,
                    pid,
                    null_mut::<*const libc::c_void>(),
                    null_mut::<*const libc::c_void>(),
                ) < 0
                {
                    let errno_message =
                        CString::from_raw(libc::strerror(*libc::__errno_location()))
                            .into_string()
                            .unwrap();
                    panic!("failed to continue: {}", errno_message,);
                }

                let mut wait_status = 0;
                let wait_options = 0;
                if libc::waitpid(pid, &mut wait_status, wait_options) < 0 {
                    let errno_message =
                        CString::from_raw(libc::strerror(*libc::__errno_location()))
                            .into_string()
                            .unwrap();
                    panic!("failed to wait on pid ({}): {}", pid, errno_message);
                }
            }
            line => {
                println!("unexpected command: \"{}\"", line);
            }
        }
    }
}

pub mod cli {
    use std::{ffi::CString, process::exit};

    use thiserror::Error;

    use crate::{ipc::Pipe, session::run_session};

    pub enum Command {
        Missing,
        Attach { pid_str: String },
        Fork { program: String, args: Vec<String> },
    }

    #[derive(Debug, Error)]
    pub enum CommandFromArgsError {
        #[error("invalid -p value")]
        CommandPidParseError(#[from] std::num::ParseIntError),
    }

    impl Command {
        pub fn from_args(args: &[String]) -> Result<Command, CommandFromArgsError> {
            if args.len() == 1 {
                return Ok(Command::Missing);
            }

            if args.len() == 3 && args[1] == "-p" {
                return Ok(Command::Attach {
                    pid_str: args[2].to_string(),
                });
            }

            return Ok(Command::Fork {
                program: args[1].to_string(),
                args: args.iter().skip(2).map(|s| s.to_string()).collect(),
            });
        }

        pub unsafe fn run(&self) -> i32 {
            match self {
                Command::Missing => {
                    println!("Missing command.");
                    return -1;
                }
                Command::Attach { pid_str } => {
                    let pid = match pid_str.parse::<libc::c_int>() {
                        Err(err) => {
                            panic!("invalid value for -p: \"{}\": {:?}", pid_str, err);
                        }
                        Ok(pid) => pid,
                    };

                    if libc::ptrace(libc::PTRACE_ATTACH, pid) < 0 {
                        let errno_message =
                            CString::from_raw(libc::strerror(*libc::__errno_location()))
                                .into_string()
                                .unwrap();
                        panic!("failed to attach to pid ({}): {}", pid, errno_message);
                    }

                    run_session(pid);
                }
                Command::Fork { program, args } => {
                    let mut pipe = Pipe::new();

                    match libc::fork() {
                        0 => {
                            // Child process
                            if libc::ptrace(libc::PTRACE_TRACEME) < 0 {
                                let errno_message =
                                    CString::from_raw(libc::strerror(*libc::__errno_location()))
                                        .into_string()
                                        .unwrap();
                                pipe.send(&format!(
                                    "failed to ptrace newly forked process: {}",
                                    errno_message,
                                ));
                                exit(-1);
                            }

                            let program_char_ptr = program.as_ptr();
                            let args_char_ptr = args
                                .iter()
                                .map(|arg| arg.as_ptr())
                                .collect::<Vec<*const u8>>()
                                .as_ptr();
                            if libc::execvp(program_char_ptr, args_char_ptr) < 0 {
                                let errno_message =
                                    CString::from_raw(libc::strerror(*libc::__errno_location()))
                                        .into_string()
                                        .unwrap();
                                pipe.send(&format!(
                                    "failed to exec newly forked process: {}",
                                    errno_message
                                ))
                            }

                            unreachable!("newly forked process should have successfully exec'ed");
                        }
                        pid => {
                            // Parent process
                            pipe.close_sender();

                            let err_str = pipe.receive();
                            if err_str.len() > 0 {
                                let mut wait_status = 0;
                                let wait_options = 0;
                                if libc::waitpid(pid, &mut wait_status, wait_options) < 0 {
                                    let errno_message = CString::from_raw(libc::strerror(
                                        *libc::__errno_location(),
                                    ))
                                    .into_string()
                                    .unwrap();
                                    panic!("failed to wait on pid ({}): {}", pid, errno_message);
                                }

                                panic!("failed to fork and trace: {}", err_str);
                            }

                            let mut wait_status = 0;
                            let wait_options = 0;
                            if libc::waitpid(pid, &mut wait_status, wait_options) < 0 {
                                let errno_message =
                                    CString::from_raw(libc::strerror(*libc::__errno_location()))
                                        .into_string()
                                        .unwrap();
                                panic!("failed to wait on pid ({}): {}", pid, errno_message);
                            }

                            run_session(pid);
                        }
                    }
                }
            }
            return 0;
        }
    }
}
