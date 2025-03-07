// use std::collections::BTreeMap;
// use std::collections::HashMap;

// use deno_core::error::CoreError;
// use deno_core::serde_v8;
// use deno_core::v8;
// use deno_core::Extension;
// use deno_core::JsRuntime;
// use deno_core::RuntimeOptions;

// use crate::handler::AvailableModels;
// use crate::types::gateway::ChatCompletionRequest;
// use crate::usage::ProviderMetrics;

// use thiserror::Error;

// #[derive(Debug, thiserror::Error)]
// pub enum ScriptError {
//     #[error("Failed to serialize JSON: {0}")]
//     SerializationError(#[from] serde_json::Error),

//     #[error("Script execution failed: {0}")]
//     ExecutionError(String),

//     #[error("Memory limit exceeded")]
//     MemoryLimitExceeded,

//     #[error("Invalid return value: {0}")]
//     InvalidReturnValue(String),
// }

// impl From<EvalError> for ScriptError {
//     fn from(err: EvalError) -> Self {
//         match err {
//             EvalError::CoreError(e) => {
//                 if e.to_string().contains("memory") {
//                     ScriptError::MemoryLimitExceeded
//                 } else {
//                     ScriptError::ExecutionError(e.to_string())
//                 }
//             }
//             EvalError::DenoSerdeError(e) => ScriptError::InvalidReturnValue(e.to_string()),
//             EvalError::JsonError(e) => ScriptError::SerializationError(e),
//         }
//     }
// }

// #[derive(Error, Debug)]
// pub enum EvalError {
//     #[error(transparent)]
//     DenoSerdeError(#[from] deno_core::serde_v8::Error),

//     #[error(transparent)]
//     CoreError(#[from] CoreError),

//     #[error(transparent)]
//     JsonError(#[from] serde_json::Error),
// }

// pub struct ScriptStrategy {}

// impl ScriptStrategy {
//     pub fn run(
//         script: &str,
//         request: &ChatCompletionRequest,
//         headers: &HashMap<String, String>,
//         models: &AvailableModels,
//         metrics: &BTreeMap<String, ProviderMetrics>,
//     ) -> Result<serde_json::Value, ScriptError> {
//         // Configure runtime options with security constraints and memory limits
//         let create_params = v8::CreateParams::default().heap_limits(0, 64 * 1024 * 1024); // Set max heap to 64MB

//         let options = RuntimeOptions {
//             extensions: vec![Extension {
//                 name: "routing",
//                 ops: vec![].into(),
//                 js_files: vec![].into(),
//                 esm_files: vec![].into(),
//                 esm_entry_point: None,
//                 lazy_loaded_esm_files: vec![].into(),
//                 enabled: true,
//                 ..Default::default()
//             }],
//             module_loader: None,    // Disable module loading
//             startup_snapshot: None, // No startup snapshot
//             shared_array_buffer_store: None,
//             create_params: Some(create_params),
//             v8_platform: None,
//             inspector: false, // Disable inspector
//             skip_op_registration: false,
//             ..Default::default()
//         };

//         let mut runtime = JsRuntime::new(options);

//         // Create a secure context with limited globals
//         let code = format!(
//             "(() => {{ 
//                 // Remove potentially dangerous globals
//                 const secureGlobals = {{}};
//                 const safeProps = ['Object', 'Array', 'Number', 'String', 'Boolean', 'Math', 'JSON'];
//                 safeProps.forEach(prop => {{ secureGlobals[prop] = globalThis[prop]; }});
                
//                 // Add our script in a secure wrapper with timeout
//                 const router = (context) => {{
//                     'use strict';
//                     try {{
//                         {script}
//                         const result = route(context);
//                         if (typeof result !== 'object') {{
//                             throw new Error('Script must return an object');
//                         }}
//                         return result;
//                     }} catch (e) {{
//                         throw new Error(`Script execution failed: ${{e.message}}`);
//                     }}
//                 }};

//                 return router;
//             }})()({{
//                 request: {},
//                 headers: {},
//                 models: {},
//                 metrics: {}
//             }});",
//             serde_json::to_string(request)?,
//             serde_json::to_string(headers)?,
//             serde_json::to_string(&models.0)?,
//             serde_json::to_string(metrics)?,
//         );

//         // Execute the script
//         let result = eval(&mut runtime, code);

//         // Explicitly drop the runtime to free V8 resources
//         drop(runtime);

//         result.map_err(Into::into)
//     }
// }

// fn eval(context: &mut JsRuntime, code: String) -> Result<serde_json::Value, EvalError> {
//     let res = context.execute_script("<anon>", code);
//     match res {
//         Ok(global) => {
//             let scope = &mut context.handle_scope();
//             let local = v8::Local::new(scope, global);
//             Ok(serde_v8::from_v8::<serde_json::Value>(scope, local)?)
//         }
//         Err(err) => Err(EvalError::CoreError(err)),
//     }
// }
