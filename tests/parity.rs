//! End-to-end parity tests for the dprint Oxfmt process plugin.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::ConfigKeyValue;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::plugins::FormatConfigId;
use dprint_core::plugins::NullCancellationToken;
use dprint_core::plugins::process::ProcessPluginCommunicator;
use dprint_core::plugins::process::ProcessPluginCommunicatorFormatRequest;

#[test]
#[ignore = "requires the built runtime worker"]
fn formats_fixtures_through_real_dprint_and_node_processes() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed creating Tokio runtime");

    runtime.block_on(async {
        let plugin = PathBuf::from(env!("CARGO_BIN_EXE_dprint-process-plugin-oxfmt"));
        assert_file_exists(&plugin, "plugin binary");
        assert_file_exists(&worker_entry_path(), "Node worker");

        let communicator = ProcessPluginCommunicator::new(&plugin, |message| {
            eprintln!("plugin stderr: {message}");
        })
        .await
        .expect("plugin process should start");

        format_fixtures(&communicator).await;
        verify_syntax_error(&communicator).await;

        communicator.shutdown().await;
    });
}

async fn format_fixtures(communicator: &ProcessPluginCommunicator) {
    let cases = [
        (
            "typescript",
            "typescript.input.ts",
            "typescript.expected.ts",
            ConfigKeyMap::new(),
            false,
        ),
        (
            "javascript",
            "javascript.input.js",
            "javascript.expected.js",
            ConfigKeyMap::new(),
            false,
        ),
        (
            "single-quote",
            "single-quote.input.ts",
            "single-quote.expected.ts",
            single_quote_config(),
            false,
        ),
        (
            "already-formatted",
            "already-formatted.input.ts",
            "already-formatted.expected.ts",
            ConfigKeyMap::new(),
            true,
        ),
    ];

    for (index, (name, input_name, expected_name, config, unchanged)) in
        cases.into_iter().enumerate()
    {
        let config_id = FormatConfigId::from_raw(
            u32::try_from(index + 1).expect("fixture index should fit in a config id"),
        );
        communicator
            .register_config(config_id, &GlobalConfiguration::default(), &config)
            .await
            .unwrap_or_else(|error| panic!("{name} config should register: {error}"));

        let input_path = fixture_path(input_name);
        let input = std::fs::read(&input_path)
            .unwrap_or_else(|error| panic!("{name} input should be readable: {error}"));
        let expected_path = fixture_path(expected_name);
        let expected = std::fs::read(&expected_path)
            .unwrap_or_else(|error| panic!("{name} expected should be readable: {error}"));
        let result = communicator
            .format_text(ProcessPluginCommunicatorFormatRequest {
                file_path: input_path,
                file_bytes: input,
                range: None,
                config_id,
                override_config: ConfigKeyMap::new(),
                on_host_format: Rc::new(|_request| Box::pin(async { Ok(None) })),
                token: Arc::new(NullCancellationToken),
            })
            .await
            .unwrap_or_else(|error| panic!("{name} format should succeed: {error}"));

        if unchanged {
            assert_eq!(result, None, "{name} should report no change");
        } else {
            assert_eq!(
                result.map(|bytes| normalize_line_endings(&bytes)),
                Some(normalize_line_endings(&expected)),
                "{name} output should match Oxfmt"
            );
        }
    }
}

async fn verify_syntax_error(communicator: &ProcessPluginCommunicator) {
    let config_id = FormatConfigId::from_raw(5);
    communicator
        .register_config(
            config_id,
            &GlobalConfiguration::default(),
            &ConfigKeyMap::new(),
        )
        .await
        .expect("error config should register");
    let file_path = fixture_path("syntax-error.input.ts");
    let error = communicator
        .format_text(ProcessPluginCommunicatorFormatRequest {
            file_path: file_path.clone(),
            file_bytes: std::fs::read(&file_path).expect("error input should be readable"),
            range: None,
            config_id,
            override_config: ConfigKeyMap::new(),
            on_host_format: Rc::new(|_request| Box::pin(async { Ok(None) })),
            token: Arc::new(NullCancellationToken),
        })
        .await
        .expect_err("syntax errors should fail formatting")
        .to_string();
    assert!(error.contains("syntax-error.input.ts"));
    assert!(error.contains("Unexpected token"));
}

fn single_quote_config() -> ConfigKeyMap {
    let mut config = ConfigKeyMap::new();
    config.insert("singleQuote".to_owned(), ConfigKeyValue::Bool(true));
    config
}

fn fixture_path(name: &str) -> PathBuf {
    let category = if name.starts_with("syntax-error") {
        "errors"
    } else {
        "basic"
    };
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(category)
        .join(name)
}

fn worker_entry_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runtime/dist/worker.js")
}

fn normalize_line_endings(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\r' {
            if bytes.get(index + 1) == Some(&b'\n') {
                index += 1;
            }
            normalized.push(b'\n');
        } else {
            normalized.push(bytes[index]);
        }
        index += 1;
    }
    normalized
}

fn assert_file_exists(path: &std::path::Path, description: &str) {
    assert!(
        path.is_file(),
        "{description} not found at {}. Build it first with `just build` and `just e2e`.",
        path.display()
    );
}
