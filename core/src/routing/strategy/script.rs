use std::collections::HashMap;

use deno_core::serde_v8;
use deno_core::v8;
use deno_core::JsRuntime;
use deno_core::RuntimeOptions;

use crate::handler::AvailableModels;
use crate::routing::RouterError;

pub struct ScriptStrategy {}

impl ScriptStrategy {
    pub fn run(
        script: &str,
        headers: HashMap<String, String>,
        models: AvailableModels,
    ) -> Result<String, RouterError> {
        let mut runtime = JsRuntime::new(RuntimeOptions::default());

        let code = format!(
            "{script}; route({{}}, {}, {});",
            serde_json::to_string(&headers).unwrap(),
            serde_json::to_string(&models.0).unwrap(),
        );

        tracing::trace!(target: "routing::script", "{code}");

        let output = eval(&mut runtime, &*Box::leak(code.into_boxed_str()));

        match output {
            Ok(serde_json::Value::String(s)) => Ok(s),
            Err(e) => Err(RouterError::ScriptError(e.to_string())),
            _ => Err(RouterError::ScriptError("router script failed".to_string())),
        }
    }
}

fn eval(context: &mut JsRuntime, code: &'static str) -> Result<serde_json::Value, String> {
    let res = context.execute_script("<anon>", code);
    match res {
        Ok(global) => {
            let scope = &mut context.handle_scope();
            let local = v8::Local::new(scope, global);
            // Deserialize a `v8` object into a Rust type using `serde_v8`,
            // in this case deserialize to a JSON `Value`.
            let deserialized_value = serde_v8::from_v8::<serde_json::Value>(scope, local);

            match deserialized_value {
                Ok(value) => Ok(value),
                Err(err) => Err(format!("Cannot deserialize value: {err:?}")),
            }
        }
        Err(err) => Err(format!("Evaling error: {err:?}")),
    }
}
