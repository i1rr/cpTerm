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

/// Stream bounded chunks; a newline-free or non-UTF-8 stream must not exhaust
/// memory or stop us from draining a child's pipe. The bounded channel applies
/// backpressure when the terminal redraws more slowly than the child writes.
async fn stream_output(
    input: impl tokio::io::AsyncRead + Unpin,
    tx: &mpsc::Sender<Action>,
) -> std::io::Result<()> {
    use tokio::io::AsyncReadExt;
    let mut reader = tokio::io::BufReader::new(input);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        if (&mut reader)
            .take(4096)
            .read_until(b'\n', &mut bytes)
            .await?
            == 0
        {
            return Ok(());
        }
        let line = String::from_utf8_lossy(&bytes)
            .trim_end_matches(['\r', '\n'])
            .chars()
            .filter(|c| !c.is_control() || *c == '\t')
            .collect();
        if tx.send(Action::TaskLine(line)).await.is_err() {
            return Ok(());
        }
    }
}

pub async fn run_task(cmd: String, cwd: PathBuf, tx: mpsc::Sender<Action>) {
    let (shell, flag) = if cfg!(windows) {
        ("cmd", "/C")
    } else {
        ("sh", "-c")
    };
    run_task_direct(shell.into(), vec![flag.into(), cmd], cwd, tx).await;
}

pub async fn run_task_direct(
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
    tx: mpsc::Sender<Action>,
) {
    let result = async {
        let mut child = tokio::process::Command::new(&program)
            .args(args)
            .current_dir(cwd)
            .kill_on_drop(true)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        tokio::try_join!(stream_output(stdout, &tx), stream_output(stderr, &tx))?;
        Ok::<_, std::io::Error>(child.wait().await?.code().unwrap_or(-1))
    };
    let action = tokio::select! {
        _ = tx.closed() => return,
        result = result => match result {
            Ok(code) => Action::TaskComplete(code),
            Err(e) => Action::TaskError(format!("{program}: {e}")),
        }
    };
    let _ = tx.send(action).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn drains_long_and_binary_output_through_a_slow_receiver() {
        let mut bytes = vec![b'x'; 100_000];
        bytes.extend_from_slice(b"\xff\nend");
        let (tx, mut rx) = mpsc::channel(1);
        let writer =
            tokio::spawn(async move { stream_output(bytes.as_slice(), &tx).await.unwrap() });
        let mut text = String::new();
        while let Some(Action::TaskLine(line)) = rx.recv().await {
            assert!(line.len() <= 4100);
            text.push_str(&line);
        }
        writer.await.unwrap();
        assert_eq!(text, format!("{}�end", "x".repeat(100_000)));
    }
}
