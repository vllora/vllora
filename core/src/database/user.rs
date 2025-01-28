pub struct ClickhouseDataUserEnvVariables {}

impl ClickhouseDataUserEnvVariables {
    pub const USER: &'static str = "CLICKHOUSE_DATA_ROOT_USER";
    pub const PASSWORD: &'static str = "CLICKHOUSE_DATA_ROOT_PASSWORD";
    pub const DATABASE: &'static str = "CLICKHOUSE_DATA_ROOT_DATABASE";
    pub const URL: &'static str = "CLICKHOUSE_DATA_HOST";
    pub const PROTOCOL: &'static str = "CLICKHOUSE_DATA_PROTOCOL";
}
