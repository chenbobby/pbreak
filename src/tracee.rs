use std::{
    ffi::{CStr, CString},
    process::exit,
    ptr::null_mut,
};

use crate::ipc::Pipe;

#[derive(PartialEq)]
enum TraceeStatus {
    Running,
    Stopped,
    Exited,
    Terminated,
}

pub struct Tracee {
    pid: libc::pid_t,
    status: TraceeStatus,
}

impl Tracee {
    // Constructs a `Tracee` by attaching to an existing PID.
    pub unsafe fn from_pid(pid: libc::pid_t) -> Tracee {
        if libc::ptrace(libc::PTRACE_ATTACH, pid) < 0 {
            let errno_message = CString::from_raw(libc::strerror(*libc::__errno_location()))
                .into_string()
                .unwrap();
            panic!("failed to attach to pid ({}): {}", pid, errno_message);
        }

        let mut tracee = Tracee {
            pid: pid,
            status: TraceeStatus::Stopped,
        };

        tracee.wait_on_signal();

        return tracee;
    }

    // Constructs a `Tracee` by executing a program.
    pub unsafe fn from_cmd(program: &str, args: &[String]) -> Tracee {
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

                let mut tracee = Tracee {
                    pid: pid,
                    status: TraceeStatus::Stopped,
                };

                tracee.wait_on_signal();

                let err_str = pipe.receive();
                if err_str.len() > 0 {
                    panic!("failed to fork and trace: {}", err_str);
                }

                return tracee;
            }
        }
    }

    pub unsafe fn wait_on_signal(&mut self) {
        let mut wait_status = 0;
        let wait_options = 0;
        if libc::waitpid(self.pid, &mut wait_status, wait_options) < 0 {
            let errno_message = CString::from_raw(libc::strerror(*libc::__errno_location()))
                .into_string()
                .unwrap();
            panic!("failed to wait on pid ({}): {}", self.pid, errno_message);
        }

        if libc::WIFSTOPPED(wait_status) {
            self.status = TraceeStatus::Stopped;
            let signal = libc::WSTOPSIG(wait_status);
            println!(
                "Process ({}) stopped with signal [{}: {:?}]",
                self.pid,
                signal,
                CStr::from_ptr(libc::strsignal(signal)),
            );
            return;
        }

        if libc::WIFEXITED(wait_status) {
            self.status = TraceeStatus::Exited;
            let exit_code = libc::WEXITSTATUS(wait_status);
            println!("Process ({}) exited with code [{}]", self.pid, exit_code);
            return;
        }

        if libc::WIFSIGNALED(wait_status) {
            self.status = TraceeStatus::Terminated;
            let signal = libc::WTERMSIG(wait_status);
            println!(
                "Process ({}) terminated with signal [{}: {:?}]",
                self.pid,
                signal,
                CStr::from_ptr(libc::strsignal(signal)),
            );
            return;
        }

        unreachable!("unexpected wait status [{}]", wait_status);
    }

    pub unsafe fn resume(&self) {
        if libc::ptrace(
            libc::PTRACE_CONT,
            self.pid,
            null_mut::<*mut libc::c_void>(),
            null_mut::<*mut libc::c_void>(),
        ) < 0
        {
            let errno_message = CString::from_raw(libc::strerror(*libc::__errno_location()))
                .into_string()
                .unwrap();
            panic!("failed to continue: {}", errno_message,);
        }
    }
}

impl Drop for Tracee {
    fn drop(&mut self) {
        if self.pid == 0 {
            return;
        }

        unsafe {
            if self.status == TraceeStatus::Running {
                libc::kill(self.pid, libc::SIGSTOP);
                self.wait_on_signal();
            }

            libc::ptrace(libc::PTRACE_DETACH, self.pid);

            libc::kill(self.pid, libc::SIGCONT);
            libc::kill(self.pid, libc::SIGKILL);
            self.wait_on_signal();
        }
    }
}
