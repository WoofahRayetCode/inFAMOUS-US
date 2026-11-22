use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

struct Install(PathBuf);

impl Drop for Install {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn update_rejects_a_branch_tip_that_changed_after_review() {
    let root = Install(
        std::env::temp_dir().join(format!("infamous-update-review-{}", std::process::id())),
    );
    fs::create_dir_all(root.0.join("installer")).unwrap();
    fs::copy(
        concat!(env!("CARGO_MANIFEST_DIR"), "/infamous.toml"),
        root.0.join("installer/infamous.toml"),
    )
    .unwrap();
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let api = format!("http://{}", server.local_addr().unwrap());
    let remote_commit = "2222222222222222222222222222222222222222";
    let serve = std::thread::spawn(move || {
        let tree = r#"{"tree":[{"path":"installer/infamous.toml","type":"blob","sha":"abc"}]}"#;
        for body in [remote_commit, tree] {
            let (mut stream, _) = server.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = BufReader::new(&stream);
            let mut line = String::new();
            while request.read_line(&mut line).unwrap() != 0 && line != "\r\n" {
                line.clear();
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let output = Command::new(env!("CARGO_BIN_EXE_infamous-installer"))
        .args(["update", "--force", "--root"])
        .arg(&root.0)
        .args([
            "--expect-commit",
            "1111111111111111111111111111111111111111",
        ])
        .env("INFAMOUS_UPDATE_API", &api)
        .env("INFAMOUS_UPDATE_RAW", &api)
        .env("NO_PROXY", "127.0.0.1")
        .env_remove("GITHUB_TOKEN")
        .output()
        .unwrap();

    let error = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(error.contains("changed since review"), "{error}");
    assert!(!root.0.join(".installer").exists());
    serve.join().unwrap();
}
