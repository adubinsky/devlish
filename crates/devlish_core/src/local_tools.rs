//! Practical local execution. Location guardrails are not OS containment or
//! signed executable admission. Explicit Devlish policy decisions run upstream.
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub struct LocalTools {
    root: PathBuf,
    directories: Vec<PathBuf>,
    path: std::ffi::OsString,
}
impl LocalTools {
    pub fn new(root: &Path, path: &std::ffi::OsStr) -> Result<Self, String> {
        let root = root
            .canonicalize()
            .map_err(|_| "local tool root unavailable")?;
        let directories: Vec<_> = std::env::split_paths(path)
            .filter(|p| p.is_absolute())
            .filter_map(|p| p.canonicalize().ok())
            .filter(|p| p.is_dir())
            .collect();
        let path = std::env::join_paths(&directories).map_err(|_| "invalid tool PATH")?;
        Ok(Self {
            root,
            directories,
            path,
        })
    }

    fn resolve(&self, request: &Value) -> Result<PathBuf, String> {
        let name = devlish_vm::tool_request::validate(request)?;
        for directory in std::iter::once(&self.root).chain(&self.directories) {
            let candidate = directory.join(name);
            let Ok(target) = candidate.canonicalize() else {
                continue;
            };
            if !(target.starts_with(&self.root)
                || self.directories.iter().any(|p| target.starts_with(p)))
            {
                return Err(
                    "tool symlink resolves outside the local folder and approved PATH directories"
                        .into(),
                );
            }
            let meta = target.metadata().map_err(|_| "tool metadata unavailable")?;
            if !meta.is_file() {
                return Err("tool is not a regular executable file".into());
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if meta.permissions().mode() & 0o111 == 0 {
                    continue;
                }
            }
            return Ok(target);
        }
        Err("tool is absent from the local folder and approved PATH directories".into())
    }

    #[cfg(not(unix))]
    pub fn run(&self, _: &Value) -> Result<Value, String> {
        Err("local program execution is currently supported on Unix only".into())
    }

    #[cfg(unix)]
    pub fn run(&self, request: &Value) -> Result<Value, String> {
        use std::{
            io::Read,
            os::{fd::AsRawFd, unix::process::CommandExt},
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        let target = self.resolve(request)?;
        // This is an inspection hash, not immutable file-handle execution.
        let bytes = devlish_audit::read_bounded(&target, devlish_audit::MAX_ARTIFACT_BYTES)?;
        let digest = crate::sha256_hex(&bytes);
        let mut command = Command::new(&target);
        let arguments: Vec<_> = request["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        command
            .args(arguments)
            .current_dir(&self.root)
            .env_clear()
            .env("PATH", &self.path)
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut child = ChildGuard(Some(
            command.spawn().map_err(|_| "local tool could not start")?,
        ));
        let process = child.0.as_mut().unwrap();
        let mut stdout = process.stdout.take().ok_or("missing stdout pipe")?;
        let mut stderr = process.stderr.take().ok_or("missing stderr pipe")?;
        for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0
            {
                return Err("cannot bound local tool output".into());
            }
        }
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut out_eof = false;
        let mut err_eof = false;
        fn drain(
            reader: &mut impl Read,
            bytes: &mut Vec<u8>,
            eof: &mut bool,
        ) -> Result<(), String> {
            if *eof {
                return Ok(());
            }
            let mut buffer = [0; 4096];
            for _ in 0..4 {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        *eof = true;
                        break;
                    }
                    Ok(n) => {
                        if bytes.len() + n > 65536 {
                            return Err("local tool output exceeded 64 KiB; output withheld".into());
                        }
                        bytes.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => return Err("local tool capture failed; output withheld".into()),
                }
            }
            Ok(())
        }
        loop {
            if Instant::now() >= deadline {
                return Err("local tool exceeded five seconds; output withheld".into());
            }
            drain(&mut stdout, &mut out, &mut out_eof)?;
            drain(&mut stderr, &mut err, &mut err_eof)?;
            // Keep the child unreaped until both EOFs, so timeout cleanup never
            // targets a reused PID while descendants retain output pipes.
            if out_eof && err_eof {
                if let Some(status) = child
                    .0
                    .as_mut()
                    .unwrap()
                    .try_wait()
                    .map_err(|_| "cannot collect local tool")?
                {
                    child.0.take();
                    if Instant::now() >= deadline {
                        return Err("local tool capture exceeded deadline".into());
                    }
                    let code = status
                        .code()
                        .ok_or("local tool terminated abnormally; output withheld")?;
                    return Ok(json!({"exit_code":code,
                        "stdout":String::from_utf8(out).map_err(|_| "tool stdout is not UTF-8; output withheld")?,
                        "stderr":String::from_utf8(err).map_err(|_| "tool stderr is not UTF-8; output withheld")?,
                        "executable":{"path":target,"inspected_sha256":digest,"profile":"local-path",
                            "immutable_execution_verified":false,"os_containment_verified":false}}));
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
#[cfg(unix)]
struct ChildGuard(Option<std::process::Child>);
#[cfg(unix)]
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
