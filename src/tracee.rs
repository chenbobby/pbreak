use std::{
    ffi::{CStr, CString},
    process::exit,
    ptr::{null, null_mut},
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
            let errno_message = CStr::from_ptr(libc::strerror(*libc::__errno_location()));
            panic!("failed to attach to pid ({}): {:?}", pid, errno_message);
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
                    let errno_message = CStr::from_ptr(libc::strerror(*libc::__errno_location()));
                    pipe.send(&format!(
                        "failed to ptrace newly forked process: {:?}",
                        errno_message,
                    ));
                    exit(-1);
                }

                let program = CString::new(program).unwrap();
                let mut args = args
                    .iter()
                    .map(|arg| {
                        let arg = CString::new(arg.as_bytes()).unwrap();
                        arg.as_ptr()
                    })
                    .collect::<Vec<*const libc::c_char>>();
                args.push(null());

                if libc::execvp(program.as_ptr(), args.as_ptr()) < 0 {
                    let errno_message =
                        CString::from_raw(libc::strerror(*libc::__errno_location()))
                            .into_string()
                            .unwrap();
                    pipe.send(&format!(
                        "failed to exec newly forked process: {:?}",
                        errno_message
                    ));
                    exit(-1);
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

                let err_str = pipe.receive();
                if err_str.len() > 0 {
                    panic!("failed to fork and trace: {}", err_str);
                }

                tracee.wait_on_signal();

                return tracee;
            }
        }
    }

    pub unsafe fn wait_on_signal(&mut self) {
        let mut wait_status = 0;
        let wait_options = 0;
        if libc::waitpid(self.pid, &mut wait_status, wait_options) < 0 {
            let errno_message = CStr::from_ptr(libc::strerror(*libc::__errno_location()));
            panic!("failed to wait on pid ({}): {:?}", self.pid, errno_message);
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

    pub unsafe fn resume(&mut self) {
        if libc::ptrace(
            libc::PTRACE_CONT,
            self.pid,
            null_mut::<*mut libc::c_void>(),
            null_mut::<*mut libc::c_void>(),
        ) < 0
        {
            let errno_message = CStr::from_ptr(libc::strerror(*libc::__errno_location()));
            panic!("failed to continue: {:?}", errno_message);
        }
        self.status = TraceeStatus::Running;
    }
}

impl Drop for Tracee {
    fn drop(&mut self) {
        if self.pid == 0 {
            return;
        }

        unsafe {
            let mut wait_status = 0;
            let wait_options = 0;
            if self.status == TraceeStatus::Running {
                libc::kill(self.pid, libc::SIGSTOP);
                libc::waitpid(self.pid, &mut wait_status, wait_options);
            }

            libc::ptrace(libc::PTRACE_DETACH, self.pid);

            libc::kill(self.pid, libc::SIGCONT);
            libc::kill(self.pid, libc::SIGKILL);
            libc::waitpid(self.pid, &mut wait_status, wait_options);
        }
    }
}

#[cfg(test)]
mod test {
    use std::{ffi::CString, io::BufRead, ptr::null};

    use super::Tracee;

    #[test]
    fn tracee_from_pid_succeeds_when_pid_exists() {
        unsafe {
            match libc::fork() {
                0 => {
                    // Child process
                    let program = CString::new("sleep").unwrap();
                    let arg = CString::new("1").unwrap();
                    let mut args = vec![arg.as_ptr()];
                    args.push(null());
                    libc::execvp(program.as_ptr(), args.as_ptr());
                }
                pid => {
                    // Parent process
                    Tracee::from_pid(pid);
                }
            }
        }
    }

    #[test]
    #[should_panic]
    fn tracee_from_pid_panics_when_pid_does_not_exist() {
        unsafe {
            Tracee::from_pid(-1);
        }
    }

    #[test]
    fn tracee_from_cmd_succeeds_when_command_is_valid() {
        unsafe {
            let tracee = Tracee::from_cmd("sleep", &vec!["1".to_string()]);
            let status = procfs_read_status(tracee.pid);
            assert_eq!('t', status);
        }
    }

    #[test]
    #[should_panic]
    fn tracee_from_cmd_panics_when_command_is_not_valid() {
        unsafe {
            Tracee::from_cmd("nonexistent_program", &vec![]);
        }
    }

    #[test]
    fn tracee_resume_succeeds_when_tracee_is_from_pid() {
        unsafe {
            match libc::fork() {
                0 => {
                    // Child process
                    let program = CString::new("sleep").unwrap();
                    let arg = CString::new("1").unwrap();
                    let mut args = vec![arg.as_ptr()];
                    args.push(null());
                    libc::execvp(program.as_ptr(), args.as_ptr());
                }
                pid => {
                    // Parent process
                    let mut tracee = Tracee::from_pid(pid);
                    tracee.resume();
                    let status = procfs_read_status(tracee.pid);
                    assert_eq!('R', status);
                }
            }
        }
    }

    #[test]
    fn tracee_resume_succeeds_when_tracee_is_from_cmd() {
        unsafe {
            let mut tracee = Tracee::from_cmd("sleep", &vec!["1".to_string()]);
            tracee.resume();
            let status = procfs_read_status(tracee.pid);
            assert_eq!('R', status);
        }
    }

    #[test]
    #[should_panic]
    fn tracee_resume_panics_when_tracee_has_existed() {
        unsafe {
            let mut tracee = Tracee::from_cmd("echo", &vec![]);
            tracee.resume();
            tracee.wait_on_signal();
            tracee.resume();
        }
    }

    fn procfs_read_status(pid: libc::pid_t) -> char {
        let procfs_path = format!("/proc/{}/stat", pid);
        let file = std::fs::File::open(procfs_path).unwrap();
        let mut file_reader = std::io::BufReader::new(file);
        let mut line = String::new();
        file_reader.read_line(&mut line).unwrap();
        let status_index = line.rfind(")").unwrap() + 2;
        return line.chars().nth(status_index).unwrap();
    }
}
