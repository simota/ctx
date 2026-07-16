use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::ExitStatus;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static DIR_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temp_repo(name: &str) -> PathBuf {
    let sequence = DIR_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ctx-diff-{name}-{}-{nanos}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&root).expect("create test repository");
    git(&root, &["init", "--quiet"]);
    git(&root, &["config", "user.email", "diff@example.com"]);
    git(&root, &["config", "user.name", "Diff Test"]);
    root
}

fn commit_files(root: &Path, files: &[(&str, &str)]) {
    for (path, contents) in files {
        let path = root.join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create fixture directory");
        }
        fs::write(path, contents).expect("write fixture");
    }
    git(root, &["add", "."]);
    git(root, &["commit", "--quiet", "-m", "initial"]);
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed with {status}");
}

fn raw_git_diff(root: &Path, paths: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "HEAD",
            "--",
        ])
        .args(paths)
        .output()
        .expect("run git diff");
    assert!(output.status.success());
    output.stdout
}

fn raw_untracked_diff(root: &Path, path: &str) -> Vec<u8> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "diff",
            "--no-index",
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--",
            "/dev/null",
            path,
        ])
        .output()
        .expect("run git diff --no-index");
    assert_eq!(output.status.code(), Some(1));
    output.stdout
}

fn ctx(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(args)
        .output()
        .expect("run ctx")
}

#[cfg(unix)]
fn wait_for_exit(child: &mut Child, timeout: Duration) -> Option<ExitStatus> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().expect("poll child status") {
            return Some(status);
        }
        if std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn one_shot_is_raw_head_diff_with_path_filter() {
    let root = temp_repo("once");
    commit_files(
        &root,
        &[
            ("tracked.txt", "base\n"),
            ("staged.txt", "base\n"),
            (".gitignore", "ignored.txt\n"),
        ],
    );

    fs::write(root.join("tracked.txt"), "unstaged\n").unwrap();
    fs::write(root.join("staged.txt"), "staged\n").unwrap();
    git(&root, &["add", "staged.txt"]);
    fs::write(root.join("staged.txt"), "staged and unstaged\n").unwrap();
    fs::write(root.join("empty.txt"), "").unwrap();
    fs::write(root.join("untracked.txt"), "included\n").unwrap();
    fs::write(root.join("ignored.txt"), "excluded\n").unwrap();

    let root_arg = root.to_str().unwrap();
    let output = ctx(&["diff", root_arg]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected = raw_git_diff(&root, &[]);
    expected.extend(raw_untracked_diff(&root, "empty.txt"));
    expected.extend(raw_untracked_diff(&root, "untracked.txt"));
    assert_eq!(output.stdout, expected);
    let patch = String::from_utf8_lossy(&output.stdout);
    assert!(patch.contains("tracked.txt"), "unstaged change is included");
    assert!(patch.contains("staged.txt"), "staged change is included");
    assert!(
        patch.contains("untracked.txt"),
        "untracked change is included"
    );
    assert!(
        patch.contains("empty.txt"),
        "empty untracked file is included"
    );
    assert!(!patch.contains("ignored.txt"), "ignored file is excluded");

    let filtered = ctx(&["diff", root_arg, "--path", "tracked.txt"]);
    assert!(filtered.status.success());
    assert_eq!(filtered.stdout, raw_git_diff(&root, &["tracked.txt"]));
    assert!(!String::from_utf8_lossy(&filtered.stdout).contains("staged.txt"));

    let untracked = ctx(&["diff", root_arg, "--path", "untracked.txt"]);
    assert!(untracked.status.success());
    assert_eq!(untracked.stdout, raw_untracked_diff(&root, "untracked.txt"));

    let repeated = ctx(&[
        "diff",
        root_arg,
        "--path",
        "tracked.txt",
        "--path",
        "staged.txt",
        "--path",
        "untracked.txt",
    ]);
    assert!(repeated.status.success());
    let mut repeated_expected =
        raw_git_diff(&root, &["tracked.txt", "staged.txt", "untracked.txt"]);
    repeated_expected.extend(raw_untracked_diff(&root, "untracked.txt"));
    assert_eq!(repeated.stdout, repeated_expected);
}

struct WatchProcess {
    child: Child,
    events: Receiver<Vec<u8>>,
}

impl WatchProcess {
    fn spawn(args: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_ctx"))
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn ctx diff --watch");
        let stdout = child.stdout.take().expect("watch stdout");
        let (sender, events) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut event = Vec::new();
            loop {
                let mut line = Vec::new();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        if line.starts_with(b"@@ ctx-diff event=") {
                            event.clear();
                        }
                        event.extend_from_slice(&line);
                        if line.starts_with(b"@@ ctx-diff end event=")
                            && sender.send(event.clone()).is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
        Self { child, events }
    }

    fn next_event(&self) -> Vec<u8> {
        self.events
            .recv_timeout(Duration::from_secs(5))
            .expect("watch event within timeout")
    }

    fn assert_no_event(&self, timeout: Duration) {
        assert!(matches!(
            self.events.recv_timeout(timeout),
            Err(RecvTimeoutError::Timeout)
        ));
    }

    #[cfg(unix)]
    fn interrupt(mut self) {
        let status = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .expect("send SIGINT");
        assert!(status.success());
        let Some(status) = wait_for_exit(&mut self.child, Duration::from_secs(2)) else {
            let _ = self.child.kill();
            let _ = self.child.wait();
            panic!("watch did not exit within 2s after Ctrl-C");
        };
        assert_eq!(status.code(), Some(0), "watch should exit 0 on Ctrl-C");
    }
}

impl Drop for WatchProcess {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn text(event: &[u8]) -> String {
    String::from_utf8(event.to_vec()).expect("event is utf-8")
}

#[test]
#[cfg(unix)]
fn watch_emits_initial_debounced_deduplicated_and_clean_events() {
    let root = temp_repo("watch");
    commit_files(
        &root,
        &[("tracked.txt", "base\n"), (".gitignore", "ignored.txt\n")],
    );
    let root_arg = root.to_str().unwrap();
    let watch = WatchProcess::spawn(&["diff", root_arg, "--watch"]);

    assert_eq!(
        text(&watch.next_event()),
        "@@ ctx-diff event=1 state=clean\n@@ ctx-diff end event=1\n"
    );

    fs::write(root.join("ignored.txt"), "ignored\n").unwrap();
    watch.assert_no_event(Duration::from_millis(350));

    fs::write(root.join("untracked.txt"), "new\n").unwrap();
    let untracked = text(&watch.next_event());
    assert!(untracked.contains("event=2 state=dirty"));
    assert!(untracked.contains("untracked.txt"));
    assert!(untracked.contains("+new"));
    assert!(!untracked.contains("ignored.txt"));

    fs::remove_file(root.join("untracked.txt")).unwrap();
    assert_eq!(
        text(&watch.next_event()),
        "@@ ctx-diff event=3 state=clean\n@@ ctx-diff end event=3\n"
    );

    fs::write(root.join("tracked.txt"), "intermediate\n").unwrap();
    std::thread::sleep(Duration::from_millis(150));
    fs::write(root.join("tracked.txt"), "settled\n").unwrap();
    let dirty = text(&watch.next_event());
    let expected = format!(
        "@@ ctx-diff event=4 state=dirty\n{}@@ ctx-diff end event=4\n",
        String::from_utf8(raw_git_diff(&root, &[])).unwrap()
    );
    assert_eq!(dirty, expected, "watch emits the complete raw snapshot");
    assert!(dirty.contains("+settled"));
    assert!(!dirty.contains("+intermediate"));
    assert!(dirty.ends_with("@@ ctx-diff end event=4\n"));

    fs::write(root.join("tracked.txt"), "base\n").unwrap();
    assert_eq!(
        text(&watch.next_event()),
        "@@ ctx-diff event=5 state=clean\n@@ ctx-diff end event=5\n"
    );
    watch.assert_no_event(Duration::from_millis(350));
    watch.interrupt();
}

#[test]
#[cfg(unix)]
fn watch_path_filter_ignores_other_tracked_files() {
    let root = temp_repo("watch-path");
    commit_files(
        &root,
        &[("included.txt", "base\n"), ("other.txt", "base\n")],
    );
    let root_arg = root.to_str().unwrap();
    let watch = WatchProcess::spawn(&[
        "diff",
        root_arg,
        "--watch",
        "--debounce",
        "50ms",
        "--path",
        "included.txt",
        "--path",
        "new.txt",
    ]);
    assert!(text(&watch.next_event()).contains("event=1 state=clean"));

    fs::write(root.join("other.txt"), "ignored\n").unwrap();
    watch.assert_no_event(Duration::from_millis(350));
    fs::write(root.join("other-untracked.txt"), "ignored\n").unwrap();
    watch.assert_no_event(Duration::from_millis(350));
    fs::write(root.join("new.txt"), "included\n").unwrap();
    let event = text(&watch.next_event());
    assert!(event.contains("event=2 state=dirty"));
    assert!(event.contains("new.txt"));
    assert!(!event.contains("other.txt"));
    assert!(!event.contains("other-untracked.txt"));
    watch.interrupt();
}

#[test]
#[cfg(unix)]
fn ctrl_c_during_initial_git_kills_child_and_exits_promptly() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = temp_repo("slow-git");
    let fake_bin = fixture.join("fake-bin");
    fs::create_dir_all(&fake_bin).unwrap();
    let fake_git = fake_bin.join("git");
    let pid_file = fixture.join("fake-git.pid");
    fs::write(
        &fake_git,
        "#!/bin/sh\necho $$ > \"$CTX_FAKE_GIT_PID\"\nexec sleep 30\n",
    )
    .unwrap();
    fs::set_permissions(&fake_git, fs::Permissions::from_mode(0o755)).unwrap();

    let path = std::iter::once(fake_bin.clone())
        .chain(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ))
        .collect::<Vec<_>>();
    let path = std::env::join_paths(path).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(["diff", fixture.to_str().unwrap(), "--watch"])
        .env("PATH", path)
        .env("CTX_FAKE_GIT_PID", &pid_file)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn watch with slow fake git");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !pid_file.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    if !pid_file.exists() {
        let _ = child.kill();
        let _ = child.wait();
        panic!("fake git did not start within 5s");
    }
    let fake_pid = fs::read_to_string(&pid_file).unwrap();
    let fake_pid = fake_pid.trim();

    let signal = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("signal ctx");
    assert!(signal.success());
    let Some(status) = wait_for_exit(&mut child, Duration::from_secs(2)) else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("ctx did not exit promptly while initial git was running");
    };
    assert_eq!(status.code(), Some(0));

    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while Command::new("kill")
        .args(["-0", fake_pid])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
        && std::time::Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(20));
    }
    let fake_is_running = Command::new("kill")
        .args(["-0", fake_pid])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    assert!(
        !fake_is_running,
        "fake git child {fake_pid} is still running"
    );
}

#[test]
fn diff_help_describes_native_arguments_and_options() {
    let output = ctx(&["diff", "--help"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let help = String::from_utf8(output.stdout).unwrap();
    for expected in [
        "Usage: ctx diff [ROOT] [OPTIONS]",
        "[ROOT]",
        "--watch",
        "--debounce <DURATION>",
        "--path <PATH>",
    ] {
        assert!(help.contains(expected), "missing {expected:?} in:\n{help}");
    }
}

#[test]
fn reports_git_and_argument_errors_with_contract_exit_codes() {
    let missing = temp_repo("missing").join("does-not-exist");
    let git_error = ctx(&["diff", missing.to_str().unwrap()]);
    assert_eq!(git_error.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&git_error.stderr).starts_with("ctx diff: "));

    let bad_duration = ctx(&["diff", "--watch", "--debounce", "soon"]);
    assert_eq!(bad_duration.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&bad_duration.stderr).starts_with("ctx diff: invalid duration"));

    let unknown = ctx(&["diff", "--unknown"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unknown.stderr).starts_with("ctx diff: unknown option"));
}

#[test]
fn rejects_non_positive_debounce() {
    for duration in ["0ms", "0"] {
        let missing = temp_repo("zero-debounce").join("does-not-exist");
        let output = ctx(&[
            "diff",
            missing.to_str().unwrap(),
            "--watch",
            "--debounce",
            duration,
        ]);
        assert_eq!(output.status.code(), Some(2), "duration {duration}");
    }
}

#[test]
fn reports_unborn_head_and_non_repository_as_git_errors() {
    let unborn = temp_repo("unborn");
    let non_repo = temp_repo("non-repo");
    fs::remove_dir_all(non_repo.join(".git")).expect("remove repository metadata");

    for root in [unborn, non_repo] {
        let output = ctx(&["diff", root.to_str().unwrap()]);
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).starts_with("ctx diff: "));
    }
}
