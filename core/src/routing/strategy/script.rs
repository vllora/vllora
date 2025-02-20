use std::collections::BTreeMap;
use std::collections::HashMap;

use deno_core::error::CoreError;
use deno_core::serde_v8;
use deno_core::v8;
use deno_core::Extension;
use deno_core::JsRuntime;
use deno_core::RuntimeOptions;

use crate::handler::AvailableModels;
use crate::types::gateway::ChatCompletionRequest;
use crate::usage::ProviderMetrics;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum ScriptError {
    #[error("Error during JS execution: {0}")]
    EvalError(#[from] EvalError),

    #[error("Serializing of context error: {0}")]
    SerdeError(#[from] serde_json::Error),
}

#[derive(Error, Debug)]
pub enum EvalError {
    #[error(transparent)]
    DenoSerdeError(#[from] deno_core::serde_v8::Error),

    #[error(transparent)]
    CoreError(#[from] CoreError),

    #[error("Invalid parameters: {0}")]
    InvalidParameters(String),
}

pub struct ScriptStrategy {}

impl ScriptStrategy {
    pub fn run(
        script: &str,
        request: &ChatCompletionRequest,
        headers: &HashMap<String, String>,
        models: &AvailableModels,
        metrics: &BTreeMap<String, ProviderMetrics>,
    ) -> Result<serde_json::Value, ScriptError> {
        // Configure runtime options with security constraints
        let options = RuntimeOptions {
            extensions: vec![Extension {
                name: "routing",
                ops: vec![].into(),
                js_files: vec![].into(),
                esm_files: vec![].into(),
                esm_entry_point: None,
                lazy_loaded_esm_files: vec![].into(),
                enabled: true,
                ..Default::default()
            }],
            module_loader: None,    // Disable module loading
            startup_snapshot: None, // No startup snapshot
            shared_array_buffer_store: None,
            create_params: None, // Use default V8 parameters
            v8_platform: None,
            inspector: false, // Disable inspector
            skip_op_registration: false,
            ..Default::default()
        };

        let mut runtime = JsRuntime::new(options);

        // Create a secure context with limited globals
        let code = format!(
            "(() => {{ 
                // Remove potentially dangerous globals
                const secureGlobals = {{}};
                const safeProps = ['Object', 'Array', 'Number', 'String', 'Boolean', 'Math', 'JSON'];
                safeProps.forEach(prop => {{ secureGlobals[prop] = globalThis[prop]; }});
                
                // Add our script in a secure wrapper
                const router = (context) => {{
                    'use strict';
                    {script}
                    return route(context);
                }};

                return router;
            }})()({{
                request: {},
                headers: {},
                models: {},
                metrics: {}
            }});",
            serde_json::to_string(request)?,
            serde_json::to_string(headers)?,
            serde_json::to_string(&models.0)?,
            serde_json::to_string(metrics)?,
        );

        tracing::trace!(target: "routing::script", "{code}");

        Ok(eval(&mut runtime, &*Box::leak(code.into_boxed_str()))?)
    }
}

fn eval(context: &mut JsRuntime, code: &'static str) -> Result<serde_json::Value, EvalError> {
    let res = context.execute_script("<anon>", code);
    match res {
        Ok(global) => {
            let scope = &mut context.handle_scope();
            let local = v8::Local::new(scope, global);
            // Deserialize a `v8` object into a Rust type using `serde_v8`,
            // in this case deserialize to a JSON `Value`.
            Ok(serde_v8::from_v8::<serde_json::Value>(scope, local)?)
        }
        Err(err) => Err(EvalError::CoreError(err)),
    }
}
