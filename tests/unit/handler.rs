use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::OnceLock;

use dprint_core::configuration::ConfigKeyValue;
use dprint_core::plugins::CancellationToken;
use dprint_core::plugins::FormatConfigId;
use dprint_core::plugins::NullCancellationToken;

use super::*;

#[derive(Debug)]
struct CancelledToken;

impl CancellationToken for CancelledToken {
    fn is_cancelled(&self) -> bool {
        true
    }

    fn wait_cancellation(&self) -> LocalBoxFuture<'static, ()> {
        Box::pin(async {})
    }
}

const FAKE_WORKER: &str = r#"
import readline from "node:readline";

const input = readline.createInterface({ input: process.stdin });
for await (const line of input) {
  const request = JSON.parse(line);
  if (request.sourceText === "diagnostic-error\n") {
    console.log(JSON.stringify({
      code: request.sourceText,
      errors: [{ severity: "Error", message: "Unexpected token", labels: [] }],
    }));
  } else if (request.sourceText.startsWith("diagnostic-non-error")) {
    const severity = request.sourceText.includes("advice") ? "Advice" : "Warning";
    console.log(JSON.stringify({
      code: `formatted-${severity.toLowerCase()}\n`,
      errors: [{ severity, message: `Example ${severity.toLowerCase()}`, labels: [] }],
    }));
  } else {
    console.log(JSON.stringify({
      code: request.sourceText.replaceAll("\r\n", "\n"),
      errors: [],
    }));
  }
}
"#;

fn test_worker_path() -> PathBuf {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let path = std::env::temp_dir().join(format!(
            "dprint-process-plugin-oxfmt-handler-test-{}.mjs",
            std::process::id()
        ));
        fs::write(&path, FAKE_WORKER).expect("fake worker should be writable");
        path
    })
    .clone()
}

fn test_handler() -> OxfmtPluginHandler {
    OxfmtPluginHandler {
        worker: Worker::for_test(PathBuf::from("node"), test_worker_path()),
    }
}

fn test_request(file_bytes: Vec<u8>) -> FormatRequest<ConfigKeyMap> {
    let mut config = ConfigKeyMap::new();
    config.insert("printWidth".to_owned(), ConfigKeyValue::Number(100));
    FormatRequest {
        file_path: PathBuf::from("src/example.ts"),
        file_bytes,
        config_id: FormatConfigId::from_raw(1),
        config: Arc::new(config),
        range: None,
        token: Arc::new(NullCancellationToken),
    }
}

fn format(handler: &OxfmtPluginHandler, request: FormatRequest<ConfigKeyMap>) -> FormatResult {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed creating test runtime")
        .block_on(handler.format(request, |_request| Box::pin(async { Ok(None) })))
}

#[test]
fn resolves_the_original_configuration() {
    let handler = test_handler();
    let mut config = ConfigKeyMap::new();
    config.insert("lineWidth".to_owned(), ConfigKeyValue::Number(100));

    let result = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("failed creating test runtime")
        .block_on(handler.resolve_config(config.clone(), GlobalConfiguration::default()));

    assert_eq!(result.config, config);
    assert_eq!(
        result.file_matching.file_extensions,
        ["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"]
    );
    assert!(result.diagnostics.is_empty());
}

#[test]
fn rejects_invalid_utf8() {
    let error =
        format(&test_handler(), test_request(vec![0xff])).expect_err("invalid UTF-8 should fail");
    assert!(error.to_string().contains("invalid utf-8"));
}

#[test]
fn range_formatting_returns_no_change() {
    let mut request = test_request(b"const value=1;\n".to_vec());
    request.range = Some(0..5);
    assert_eq!(format(&test_handler(), request).unwrap(), None);
}

#[test]
fn cancellation_returns_no_change() {
    let mut request = test_request(b"const value=1;\n".to_vec());
    request.token = Arc::new(CancelledToken);
    assert_eq!(format(&test_handler(), request).unwrap(), None);
}

#[test]
fn unchanged_output_returns_no_change() {
    let request = test_request(b"unchanged\n".to_vec());
    assert_eq!(format(&test_handler(), request).unwrap(), None);
}

#[test]
fn returns_oxfmt_line_endings_without_restoring_crlf() {
    let request = test_request(b"const value=1;\r\n".to_vec());
    assert_eq!(
        format(&test_handler(), request).unwrap(),
        Some(b"const value=1;\n".to_vec())
    );
}

#[test]
fn error_diagnostics_fail_but_warnings_and_advice_keep_output() {
    let error = format(
        &test_handler(),
        test_request(b"diagnostic-error\n".to_vec()),
    )
    .expect_err("error diagnostics should fail");
    assert!(error.to_string().contains("Unexpected token"));
    assert!(error.to_string().contains("src/example.ts"));

    for (source, expected) in [
        ("diagnostic-non-error-warning\n", "formatted-warning\n"),
        ("diagnostic-non-error-advice\n", "formatted-advice\n"),
    ] {
        assert_eq!(
            format(&test_handler(), test_request(source.as_bytes().to_vec())).unwrap(),
            Some(expected.as_bytes().to_vec())
        );
    }
}
