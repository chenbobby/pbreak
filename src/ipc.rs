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
                let errno_message = CString::from_raw(libc::strerror(*libc::__errno_location()))
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
                let errno_message = CString::from_raw(libc::strerror(*libc::__errno_location()))
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
