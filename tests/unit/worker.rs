use std::fs;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use dprint_core::configuration::ConfigKeyValue;
use tokio::runtime::Builder;

use super::*;

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

const FAKE_WORKER: &str = r#"
import readline from "node:readline";

const input = readline.createInterface({ input: process.stdin });
let count = 0;
for await (const line of input) {
  const request = JSON.parse(line);
  count += 1;
  if (request.sourceText === "exit") process.exit(0);
  if (request.sourceText === "malformed") {
    console.log("{");
    continue;
  }
  if (request.sourceText === "throw") {
    console.log(JSON.stringify({ error: "thrown failure" }));
    continue;
  }
  console.log(JSON.stringify({
    code: request.sourceText.startsWith("inspect") ? JSON.stringify(request) : `${request.sourceText}:${count}`,
    errors: [],
  }));
}
"#;

struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "dprint-process-plugin-oxfmt-worker-test-{}-{id}.mjs",
            std::process::id()
        ));
        fs::write(&path, FAKE_WORKER).expect("fake worker should be writable");
        Self { path }
    }

    fn worker(&self) -> Worker {
        Worker::for_test(PathBuf::from("node"), self.path.clone())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn runtime() -> tokio::runtime::Runtime {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime should build")
}

#[test]
fn forwards_relative_paths_crlf_content_and_nested_options_exactly() {
    let fixture = Fixture::new();
    let worker = fixture.worker();
    let mut nested = ConfigKeyMap::new();
    nested.insert("enabled".to_owned(), ConfigKeyValue::Bool(true));
    nested.insert(
        "items".to_owned(),
        ConfigKeyValue::Array(vec![
            ConfigKeyValue::Number(1),
            ConfigKeyValue::String("two".to_owned()),
            ConfigKeyValue::Null,
        ]),
    );
    let mut options = ConfigKeyMap::new();
    options.insert("futureOption".to_owned(), ConfigKeyValue::Object(nested));
    let runtime = runtime();

    let result = runtime
        .block_on(worker.format(Path::new("src/example.ts"), "inspect\r\n", &options))
        .expect("format should succeed");
    let request: serde_json::Value =
        serde_json::from_str(&result.code).expect("echoed request should be JSON");
    assert_eq!(
        request,
        serde_json::json!({
            "fileName": "src/example.ts",
            "sourceText": "inspect\r\n",
            "options": {
                "futureOption": {
                    "enabled": true,
                    "items": [1, "two", null]
                }
            }
        })
    );
}

#[test]
fn reuses_one_worker_for_sequential_requests() {
    let fixture = Fixture::new();
    let worker = fixture.worker();
    let options = ConfigKeyMap::new();
    let runtime = runtime();

    assert_eq!(
        runtime
            .block_on(worker.format(Path::new("first.ts"), "first", &options))
            .unwrap()
            .code,
        "first:1"
    );
    assert_eq!(
        runtime
            .block_on(worker.format(Path::new("second.ts"), "second", &options))
            .unwrap()
            .code,
        "second:2"
    );
}

#[test]
fn restarts_after_transport_failure() {
    let fixture = Fixture::new();
    let worker = fixture.worker();
    let options = ConfigKeyMap::new();
    let runtime = runtime();

    let error = runtime
        .block_on(worker.format(Path::new("exit.ts"), "exit", &options))
        .expect_err("EOF should fail");
    assert!(matches!(error, WorkerError::Eof));
    assert_eq!(
        runtime
            .block_on(worker.format(Path::new("next.ts"), "next", &options))
            .unwrap()
            .code,
        "next:1"
    );
}

#[test]
fn malformed_and_remote_responses_do_not_restart_the_worker() {
    let fixture = Fixture::new();
    let worker = fixture.worker();
    let options = ConfigKeyMap::new();
    let runtime = runtime();

    let malformed = runtime
        .block_on(worker.format(Path::new("bad.ts"), "malformed", &options))
        .expect_err("malformed response should fail");
    assert!(matches!(malformed, WorkerError::Json(_)));
    assert_eq!(
        runtime
            .block_on(worker.format(Path::new("next.ts"), "next", &options))
            .unwrap()
            .code,
        "next:2"
    );

    let remote = runtime
        .block_on(worker.format(Path::new("throw.ts"), "throw", &options))
        .expect_err("remote error should fail");
    assert!(matches!(remote, WorkerError::Remote(message) if message == "thrown failure"));
    assert_eq!(
        runtime
            .block_on(worker.format(Path::new("last.ts"), "last", &options))
            .unwrap()
            .code,
        "last:4"
    );
}

#[test]
fn packaged_worker_precedes_the_debug_fallback() {
    let executable = env::current_exe().expect("test executable path should be available");
    let packaged_worker = executable
        .parent()
        .expect("test executable should have a parent")
        .join("runtime/dist/worker.js");
    let packaged_worker_dir = packaged_worker
        .parent()
        .expect("packaged worker should have a parent");
    fs::create_dir_all(packaged_worker_dir).expect("packaged worker directory should be writable");
    let previous_contents = fs::read(&packaged_worker).ok();
    fs::write(&packaged_worker, b"packaged worker").expect("packaged worker should be writable");

    let discovered_entry = Worker::discover().map(|worker| worker.entry);

    match previous_contents {
        Some(contents) => fs::write(&packaged_worker, contents)
            .expect("existing packaged worker should be restored"),
        None => {
            fs::remove_file(&packaged_worker).expect("temporary packaged worker should be removed");
        }
    }
    let discovered_entry = discovered_entry.expect("packaged worker should be discovered");
    assert_eq!(discovered_entry, packaged_worker);
}
