use bytemuck::TransparentWrapper;
use serde_json::Value;
use valuable::{Listable, Mappable, Valuable, Visit};

pub use layer::{config, layer, level_layer, RecordResult, UuidIdGenerator};

mod layer;
pub mod span;

pub const SPAN_QUERY: &str = "query";

pub const SPAN_QUERY_ENTITIES: &str = "query_entities";

pub const SPAN_API_STREAM: &str = "api_stream";

pub const SPAN_API_INVOKE: &str = "api_invoke";

pub const SPAN_OPENAI: &str = "openai";

pub const SPAN_GEMINI: &str = "gemini";

pub const SPAN_ANTHROPIC: &str = "anthropic";

pub const SPAN_BEDROCK: &str = "bedrock";

pub const SPAN_CACHE: &str = "cache";

pub const SPAN_TOOLS: &str = "tools";

pub const SPAN_TOOL: &str = "tool";

pub const SPAN_MODEL_CALL: &str = "model_call";

pub const SPAN_OPENAI_SPEC: &str = "openai_spec";

pub const SPAN_REQUEST_ROUTING: &str = "request_routing";

pub const SPAN_GUARD_EVAULATION: &str = "guard_evaluation";

pub const SPAN_VIRTUAL_MODEL: &str = "virtual_model";

#[repr(transparent)]
pub struct JsonValue<'a>(pub &'a Value);

#[repr(transparent)]
pub struct JsonValueOwned(pub Value);

#[derive(TransparentWrapper)]
#[repr(transparent)]
struct JsonMap(serde_json::Map<String, Value>);

#[derive(TransparentWrapper)]
#[repr(transparent)]
struct JsonArray(Vec<Value>);

impl Valuable for JsonValueOwned {
    fn as_value(&self) -> valuable::Value<'_> {
        match self.0 {
            Value::Array(ref array) => JsonArray::wrap_ref(array).as_value(),
            Value::Bool(ref value) => value.as_value(),
            Value::Number(ref num) => {
                if num.is_f64() {
                    valuable::Value::F64(num.as_f64().unwrap())
                } else if num.is_i64() {
                    valuable::Value::I64(num.as_i64().unwrap())
                } else {
                    unreachable!()
                }
            }
            Value::Null => valuable::Value::Unit,
            Value::String(ref s) => s.as_value(),
            Value::Object(ref object) => JsonMap::wrap_ref(object).as_value(),
        }
    }

    fn visit(&self, visit: &mut dyn Visit) {
        match self.0 {
            Value::Array(ref array) => JsonArray::wrap_ref(array).visit(visit),
            Value::Bool(ref value) => value.visit(visit),
            Value::Number(ref num) => {
                if num.is_f64() {
                    num.as_f64().unwrap().visit(visit)
                } else if num.is_i64() {
                    num.as_i64().unwrap().visit(visit)
                } else {
                    unreachable!()
                }
            }
            Value::Null => valuable::Value::Unit.visit(visit),
            Value::String(ref s) => s.visit(visit),
            Value::Object(ref object) => JsonMap::wrap_ref(object).visit(visit),
        }
    }
}
impl Valuable for JsonValue<'_> {
    fn as_value(&self) -> valuable::Value<'_> {
        match self.0 {
            Value::Array(ref array) => JsonArray::wrap_ref(array).as_value(),
            Value::Bool(ref value) => value.as_value(),
            Value::Number(ref num) => {
                if num.is_f64() {
                    valuable::Value::F64(num.as_f64().unwrap())
                } else if num.is_i64() {
                    valuable::Value::I64(num.as_i64().unwrap())
                } else {
                    unreachable!()
                }
            }
            Value::Null => valuable::Value::Unit,
            Value::String(ref s) => s.as_value(),
            Value::Object(ref object) => JsonMap::wrap_ref(object).as_value(),
        }
    }

    fn visit(&self, visit: &mut dyn Visit) {
        match self.0 {
            Value::Array(ref array) => JsonArray::wrap_ref(array).visit(visit),
            Value::Bool(ref value) => value.visit(visit),
            Value::Number(ref num) => {
                if num.is_f64() {
                    num.as_f64().unwrap().visit(visit)
                } else if num.is_i64() {
                    num.as_i64().unwrap().visit(visit)
                } else {
                    unreachable!()
                }
            }
            Value::Null => valuable::Value::Unit.visit(visit),
            Value::String(ref s) => s.visit(visit),
            Value::Object(ref object) => JsonMap::wrap_ref(object).visit(visit),
        }
    }
}

impl Valuable for JsonMap {
    fn as_value(&self) -> valuable::Value<'_> {
        valuable::Value::Mappable(self)
    }

    fn visit(&self, visit: &mut dyn Visit) {
        for (k, v) in self.0.iter() {
            visit.visit_entry(k.as_value(), JsonValue(v).as_value());
        }
    }
}

impl Mappable for JsonMap {
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.len();
        (len, Some(len))
    }
}

impl Valuable for JsonArray {
    fn as_value(&self) -> valuable::Value<'_> {
        valuable::Value::Listable(self)
    }

    fn visit(&self, visit: &mut dyn Visit) {
        for v in self.0.iter() {
            visit.visit_value(JsonValue(v).as_value())
        }
    }
}

impl Listable for JsonArray {
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.0.len();
        (len, Some(len))
    }
}
