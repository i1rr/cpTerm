use std::path::PathBuf;

use tokio::io::AsyncBufReadExt;
use tokio::sync::mpsc;

use crate::action::Action;

pub const MAX_LINES: usize = 5000;

#[derive(Debug)]
pub struct TaskState {
    pub cmd: String,
    pub lines: Vec<String>,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub scroll: u16,
    pub auto_scroll: bool,
}

impl TaskState {
    pub fn new(cmd: impl Into<String>) -> Self {
        Self {
            cmd: cmd.into(),
            lines: Vec::new(),
            running: true,
            exit_code: None,
            scroll: 0,
            auto_scroll: true,
        }
    }

    /// Push a line of output. Auto-scrolls to the bottom unless the user
    /// has scrolled up manually.
    pub fn push_line(&mut self, line: String) {
        if self.lines.len() >= MAX_LINES {
            self.lines.remove(0);
            self.scroll = self.scroll.saturating_sub(1);
        }
        self.lines.push(line);
        if self.auto_scroll {
            self.scroll = (self.lines.len() as u16).saturating_sub(1);
        }
    }

    pub fn finish(&mut self, exit_code: i32) {
        self.running = false;
        self.exit_code = Some(exit_code);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
        self.auto_scroll = false;
    }

    pub fn scroll_down(&mut self) {
        let max = (self.lines.len() as u16).saturating_sub(1);
        self.scroll = (self.scroll + 1).min(max);
        if self.scroll >= max {
            self.auto_scroll = true;
        }
    }

    /// Short status string shown in the window title.
    pub fn exit_summary(&self) -> String {
        match (self.running, self.exit_code) {
            (true, _) => "running...".to_string(),
            (false, Some(0)) | (false, None) => "done".to_string(),
            (false, Some(n)) => format!("exit code {}", n),
        }
    }
}

/// Spawn a shell command, streaming every line to `tx` as `Action::TaskLine`.
/// Sends `Action::TaskComplete(code)` when the process exits, or
/// `Action::TaskError(msg)` if the process cannot be started.
pub async fn run_task(cmd: String, cwd: PathBuf, tx: mpsc::UnboundedSender<Action>) {
    let (shell, flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };

    let mut child = match tokio::process::Command::new(shell)
        .arg(flag)
        .arg(&cmd)
        .current_dir(&cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            let _ = tx.send(Action::TaskError(format!("failed to start '{}': {}", cmd, e)));
            return;
        }
    };

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let tx_out = tx.clone();
    let stdout_task = tokio::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx_out.send(Action::TaskLine(line)).is_err() {
                break;
            }
        }
    });

    let tx_err = tx.clone();
    let stderr_task = tokio::spawn(async move {
        let mut lines = tokio::io::BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if tx_err.send(Action::TaskLine(line)).is_err() {
                break;
            }
        }
    });

    let _ = tokio::join!(stdout_task, stderr_task);
    let exit_code = child
        .wait()
        .await
        .ok()
        .and_then(|s| s.code())
        .unwrap_or(-1);
    let _ = tx.send(Action::TaskComplete(exit_code));
}
