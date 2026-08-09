use dprint_core::async_runtime::LocalBoxFuture;
use dprint_core::async_runtime::async_trait;
use dprint_core::configuration::ConfigKeyMap;
use dprint_core::configuration::GlobalConfiguration;
use dprint_core::plugins::AsyncPluginHandler;
use dprint_core::plugins::FileMatchingInfo;
use dprint_core::plugins::FormatError;
use dprint_core::plugins::FormatRequest;
use dprint_core::plugins::FormatResult;
use dprint_core::plugins::HostFormatRequest;
use dprint_core::plugins::PluginInfo;
use dprint_core::plugins::PluginResolveConfigurationResult;

use crate::worker::DiagnosticSeverity;
use crate::worker::Worker;

const JAVASCRIPT_AND_TYPESCRIPT_EXTENSIONS: &[&str] =
    &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"];

pub struct OxfmtPluginHandler {
    worker: Worker,
}

impl OxfmtPluginHandler {
    pub fn new() -> Result<Self, FormatError> {
        Ok(Self {
            worker: Worker::discover().map_err(FormatError::new)?,
        })
    }
}

#[async_trait(?Send)]
impl AsyncPluginHandler for OxfmtPluginHandler {
    type Configuration = ConfigKeyMap;

    fn plugin_info(&self) -> PluginInfo {
        PluginInfo {
            name: env!("CARGO_PKG_NAME").to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            config_key: "oxfmt".to_owned(),
            help_url: "https://github.com/muyu258/dprint-process-plugin-oxfmt".to_owned(),
            config_schema_url:
                "https://github.com/muyu258/dprint-process-plugin-oxfmt/blob/main/schema/plugin.schema.json"
                    .to_owned(),
            update_url: None,
        }
    }

    fn license_text(&self) -> String {
        include_str!("../LICENSE").to_owned()
    }

    async fn resolve_config(
        &self,
        config: ConfigKeyMap,
        _global_config: GlobalConfiguration,
    ) -> PluginResolveConfigurationResult<Self::Configuration> {
        PluginResolveConfigurationResult {
            file_matching: FileMatchingInfo {
                file_extensions: JAVASCRIPT_AND_TYPESCRIPT_EXTENSIONS
                    .iter()
                    .map(|extension| (*extension).to_owned())
                    .collect(),
                file_names: Vec::new(),
            },
            diagnostics: Vec::new(),
            config,
        }
    }

    async fn format(
        &self,
        request: FormatRequest<Self::Configuration>,
        _format_with_host: impl FnMut(HostFormatRequest) -> LocalBoxFuture<'static, FormatResult>
        + 'static,
    ) -> FormatResult {
        if request.range.is_some() || request.token.is_cancelled() {
            return Ok(None);
        }

        let source_text = String::from_utf8(request.file_bytes)?;
        let output = self
            .worker
            .format(&request.file_path, &source_text, &request.config)
            .await
            .map_err(FormatError::new)?;

        if request.token.is_cancelled() {
            return Ok(None);
        }

        let errors = output
            .errors
            .iter()
            .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            return Err(FormatError::new(format!(
                "Oxfmt failed for {}: {}",
                request.file_path.display(),
                errors.join("; ")
            )));
        }

        if output.code == source_text {
            Ok(None)
        } else {
            Ok(Some(output.code.into_bytes()))
        }
    }
}

#[cfg(test)]
#[path = "../tests/unit/handler.rs"]
mod tests;
