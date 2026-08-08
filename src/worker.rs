use std::env;
use std::fmt::Display;
use std::fmt::Formatter;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;

use dprint_core::configuration::ConfigKeyMap;
use serde::Deserialize;
use serde::Serialize;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;
use tokio::sync::Mutex;

const NODE_OVERRIDE_ENV: &str = "DPRINT_OXFMT_NODE";
const WORKER_OVERRIDE_ENV: &str = "DPRINT_OXFMT_WORKER";

pub(crate) struct Worker {
    node_program: PathBuf,
    entry: PathBuf,
    session: Mutex<Option<WorkerSession>>,
}

impl Worker {
    pub(crate) fn discover() -> Result<Self, WorkerError> {
        let node_program =
            env::var_os(NODE_OVERRIDE_ENV).map_or_else(|| PathBuf::from("node"), PathBuf::from);

        let worker_entry = {
            let candidates = env::var_os(WORKER_OVERRIDE_ENV).map_or_else(
                || {
                    let mut candidates = env::current_exe()
                        .ok()
                        .and_then(|executable| {
                            executable
                                .parent()
                                .map(|parent| vec![parent.join("runtime/dist/worker.js")])
                        })
                        .unwrap_or_default();

                    if cfg!(debug_assertions) {
                        candidates.push(
                            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                                .join("runtime/dist/worker.js"),
                        );
                    }

                    candidates
                },
                |worker_override| vec![PathBuf::from(worker_override)],
            );

            let Some(worker_entry) = candidates
                .iter()
                .find(|candidate| candidate.is_file())
                .cloned()
            else {
                return Err(WorkerError::Discovery { candidates });
            };

            worker_entry
        };
        Ok(Self::new(node_program, worker_entry))
    }

    fn new(node_program: PathBuf, entry: PathBuf) -> Self {
        Self {
            node_program,
            entry,
            session: Mutex::new(None),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(node_program: PathBuf, worker_entry: PathBuf) -> Self {
        Self::new(node_program, worker_entry)
    }

    pub(crate) async fn format(
        &self,
        file_name: &Path,
        source_text: &str,
        options: &ConfigKeyMap,
    ) -> Result<FormatResult, WorkerError> {
        let request = serde_json::to_string(&FormatRequest {
            file_name: file_name.to_string_lossy().as_ref(),
            source_text,
            options,
        })?;
        let mut session = self.session.lock().await;
        if session.is_none() {
            *session = Some(WorkerSession::spawn(&self.node_program, &self.entry)?);
        }

        let result = session
            .as_mut()
            .expect("worker session was initialized")
            .request(&request)
            .await;
        if result.as_ref().is_err_and(WorkerError::is_transport) {
            session.take();
        }
        result
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FormatRequest<'a> {
    file_name: &'a str,
    source_text: &'a str,
    options: &'a ConfigKeyMap,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum WorkerResponse {
    Result(FormatResult),
    Error { error: String },
}

#[derive(Debug, Deserialize)]
pub(crate) struct FormatResult {
    pub(crate) code: String,
    pub(crate) errors: Vec<FormatDiagnostic>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct FormatDiagnostic {
    pub(crate) severity: DiagnosticSeverity,
    pub(crate) message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub(crate) enum DiagnosticSeverity {
    Error,
    Warning,
    Advice,
}

#[derive(Debug)]
pub(crate) enum WorkerError {
    Discovery { candidates: Vec<PathBuf> },
    Transport(std::io::Error),
    Eof,
    Json(serde_json::Error),
    Remote(String),
}

impl WorkerError {
    fn is_transport(&self) -> bool {
        matches!(self, Self::Transport(_) | Self::Eof)
    }
}

impl Display for WorkerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Discovery { candidates } => write!(
                formatter,
                "could not locate the Oxfmt worker; checked: {}",
                candidates
                    .iter()
                    .map(|candidate| candidate.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Transport(error) => write!(formatter, "Oxfmt worker transport failed: {error}"),
            Self::Eof => formatter.write_str("Oxfmt worker exited before sending a response"),
            Self::Json(error) => write!(formatter, "Oxfmt worker returned invalid JSON: {error}"),
            Self::Remote(error) => write!(formatter, "Oxfmt worker failed: {error}"),
        }
    }
}

impl std::error::Error for WorkerError {}

impl From<std::io::Error> for WorkerError {
    fn from(error: std::io::Error) -> Self {
        Self::Transport(error)
    }
}

impl From<serde_json::Error> for WorkerError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

struct WorkerSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl WorkerSession {
    fn spawn(node_program: &Path, worker_entry: &Path) -> Result<Self, WorkerError> {
        let mut child = Command::new(node_program)
            .arg(worker_entry)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take().ok_or_else(|| {
            WorkerError::Transport(std::io::Error::other("worker stdin was not piped"))
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            WorkerError::Transport(std::io::Error::other("worker stdout was not piped"))
        })?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    async fn request(&mut self, request: &str) -> Result<FormatResult, WorkerError> {
        self.stdin.write_all(request.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;

        let mut line = String::new();
        if self.stdout.read_line(&mut line).await? == 0 {
            return Err(WorkerError::Eof);
        }
        match serde_json::from_str(&line)? {
            WorkerResponse::Result(result) => Ok(result),
            WorkerResponse::Error { error } => Err(WorkerError::Remote(error)),
        }
    }
}

impl Drop for WorkerSession {
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

#[cfg(test)]
#[path = "../tests/unit/worker.rs"]
mod tests;
