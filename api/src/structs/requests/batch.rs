#[derive(Debug, Clone, serde::Deserialize)]
pub struct BatchRequest {
    /// The OHS project this request is for.
    pub project: String,

    /// The request method (e.g. `get_write_key`, `register`, etc.)
    pub method: String,

    /// The request data, which can be any JSON value.
    pub data: serde_json::Value,
}
