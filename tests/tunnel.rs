//! Real-binary SSH CONNECT failure characterizations (SPEC-0018 T-1803).

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const TARGET: &str = "ssh.example.test:2222";
const PROXY_URL_USERINFO: &str = "sandme:fixture-secret";
const EXPECTED_BASIC: &str = "Basic c2FuZG1lOmZpeHR1cmUtc2VjcmV0";
const DEADLINE: Duration = Duration::from_secs(5);

struct HelperScratch(PathBuf);

impl HelperScratch {
    fn new(case: &str) -> Self {
        let base =
            std::env::temp_dir().join(format!("sandme-tunnel-{}-{case}", std::process::id()));
        for candidate in 0..32 {
            let path = if candidate == 0 {
                base.clone()
            } else {
                base.with_file_name(format!(
                    "sandme-tunnel-{}-{case}-{candidate}",
                    std::process::id()
                ))
            };
            match fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create isolated helper output directory: {error}"),
            }
        }
        panic!("exhausted isolated helper output directory candidates for {case}")
    }
}

impl Drop for HelperScratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ReapedChild(Child);

impl Drop for ReapedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn bounded_output(path: &Path) -> Vec<u8> {
    let file = File::open(path).expect("open captured helper output");
    assert!(
        file.metadata().unwrap().len() <= 16 * 1024,
        "helper output exceeded 16 KiB: {}",
        path.display()
    );
    let mut output = Vec::new();
    file.take(16 * 1024 + 1).read_to_end(&mut output).unwrap();
    output
}

fn run_ssh_helper(case: &str, proxy_url: Option<&str>) -> Output {
    let scratch = HelperScratch::new(case);
    let stdout = scratch.0.join("stdout");
    let stderr = scratch.0.join("stderr");
    let mut command = Command::new(env!("CARGO_BIN_EXE_sandme"));
    command
        .args(["--sandme-ssh-connect", "ssh.example.test", "2222"])
        .env_remove("HTTP_PROXY")
        .env_remove("http_proxy")
        .env("SANDME_PROXY_PORT", "not-a-port")
        .stdin(Stdio::null())
        .stdout(File::create(&stdout).unwrap())
        .stderr(File::create(&stderr).unwrap());
    if let Some(url) = proxy_url {
        command.env("HTTP_PROXY", url);
    }
    let mut child = ReapedChild(command.spawn().expect("launch actual sandme binary"));
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().expect("poll sandme helper") {
            break status;
        }
        assert!(
            started.elapsed() < DEADLINE,
            "sandme helper exceeded 5 seconds"
        );
        thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: bounded_output(&stdout),
        stderr: bounded_output(&stderr),
    }
}

fn accept_client(listener: &TcpListener, stop: &Receiver<()>) -> io::Result<std::net::TcpStream> {
    let started = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if stop.try_recv().is_ok() {
                    return Err(io::Error::new(
                        io::ErrorKind::Interrupted,
                        "fixture cancelled",
                    ));
                }
                if started.elapsed() >= DEADLINE {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "CONNECT not received",
                    ));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => return Err(error),
        }
    }
}

fn answer_refused_connect(
    listener: &TcpListener,
    status: u16,
    stop: &Receiver<()>,
) -> io::Result<()> {
    let mut stream = accept_client(listener, stop)?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let started = Instant::now();
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        if started.elapsed() >= DEADLINE {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "CONNECT header timeout",
            ));
        }
        assert!(head.len() < 4096, "CONNECT header exceeded 4096 bytes");
        let mut byte = [0_u8];
        stream.read_exact(&mut byte)?;
        head.push(byte[0]);
    }
    assert_eq!(
        head,
        format!(
            "CONNECT {TARGET} HTTP/1.1\r\nHost: {TARGET}\r\nProxy-Authorization: {EXPECTED_BASIC}\r\n\r\n"
        )
        .as_bytes(),
        "fixture must receive the exact CONNECT target and Basic authorization"
    );
    stream
        .write_all(format!("HTTP/1.1 {status} Refused\r\nContent-Length: 0\r\n\r\n").as_bytes())?;
    eprintln!("fixture verified CONNECT target and Basic header; answered HTTP {status}");
    Ok(())
}

struct RefusalFixture {
    url: String,
    stop: Sender<()>,
    handle: Option<JoinHandle<io::Result<()>>>,
}

impl RefusalFixture {
    fn start(status: u16) -> Self {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!(
            "http://{PROXY_URL_USERINFO}@{}",
            listener.local_addr().unwrap()
        );
        let (stop, receiver) = mpsc::channel();
        let handle = thread::spawn(move || answer_refused_connect(&listener, status, &receiver));
        Self {
            url,
            stop,
            handle: Some(handle),
        }
    }

    fn join(mut self) {
        self.handle
            .take()
            .expect("fixture handle is owned")
            .join()
            .expect("fixture assertions must not panic")
            .expect("fixture socket exchange must complete");
    }
}

impl Drop for RefusalFixture {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[test]
fn reports_an_ssh_tunnel_without_a_usable_proxy() {
    // Given no HTTP_PROXY or http_proxy, and an invalid normal proxy-port setting
    // When the real binary is invoked through the private SSH helper sentinel
    let output = run_ssh_helper("no-proxy", None);

    // Then early helper dispatch reports its own failure, without normal config startup
    assert_eq!(output.status.code(), Some(125));
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("sandme: SSH tunnel: no usable HTTP_PROXY in the environment"),
        "unexpected diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn reports_a_refused_ssh_connect_with_the_reserved_status() {
    // Given a local proxy that verifies CONNECT and Basic before refusing both status classes
    for status in [403, 407] {
        let fixture = RefusalFixture::start(status);

        // When the real SSH helper sends the exact destination through that proxy
        let output = run_ssh_helper(&format!("refused-{status}"), Some(&fixture.url));
        fixture.join(); // Propagate the socket result and fixture assertions before judging the client.

        // Then each refusal reports sandme's reserved status, no stdout and its own target/status
        assert_eq!(output.status.code(), Some(125), "HTTP {status}");
        assert_eq!(output.stdout, Vec::<u8>::new(), "HTTP {status}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(&format!(
                "sandme: SSH tunnel: proxy declined CONNECT to {TARGET} (HTTP {status})"
            )),
            "HTTP {status} diagnostic: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn separate_ssh_capture_preserves_an_existing_owners_output() {
    // Given two previous owners' capture directories for this case
    let case = "occupied-capture";
    let prior_owner = HelperScratch::new(case);
    let prior_marker = prior_owner.0.join("stdout");
    fs::write(&prior_marker, b"prior owner's output").unwrap();
    let old_owner = HelperScratch::new(case);
    let old_path = old_owner.0.clone();
    let marker = old_path.join("stdout");
    fs::write(&marker, b"previous owner's output").unwrap();

    // When a separate capture is reserved and the real SSH helper runs without a proxy
    let fresh_owner = HelperScratch::new(case);
    assert_ne!(fresh_owner.0, old_owner.0);
    assert_eq!(fs::read(&prior_marker).unwrap(), b"prior owner's output");
    assert_eq!(fs::read(&marker).unwrap(), b"previous owner's output");
    let output = run_ssh_helper(case, None);
    assert_eq!(fs::read(&prior_marker).unwrap(), b"prior owner's output");
    assert_eq!(fs::read(&marker).unwrap(), b"previous owner's output");
    let fresh_path = fresh_owner.0.clone();
    drop(fresh_owner);

    // Then the binary reports its own failure, only the fresh capture is cleaned up,
    // and the previous owner's output remains unchanged throughout
    assert_eq!(output.status.code(), Some(125));
    assert_eq!(output.stdout, Vec::<u8>::new());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("sandme: SSH tunnel: no usable HTTP_PROXY in the environment")
    );
    assert!(!fresh_path.exists());
    assert_eq!(fs::read(&prior_marker).unwrap(), b"prior owner's output");
    assert_eq!(fs::read(&marker).unwrap(), b"previous owner's output");
}
