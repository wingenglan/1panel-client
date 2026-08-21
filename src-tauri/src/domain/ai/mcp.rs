use crate::errors::{AppError, AppResult};
use crate::infra::local::LocalRepository;
use crate::security::CredentialStore;
use chrono::{DateTime, Utc};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;
use uuid::Uuid;

const SETTINGS_KEY: &str = "ai.mcp.servers";
const MAX_SERVERS: usize = 32;
const MAX_ARGS: usize = 64;
const MAX_TOOLS_PER_SERVER: usize = 128;
const MAX_TOOL_RESULT_BYTES: usize = 64 * 1024;
const DEFAULT_TIMEOUT_SECONDS: u64 = 15;
const MAX_URL_LENGTH: usize = 2048;
const MAX_AUTH_TOKEN_LENGTH: usize = 4096;
const AUTH_KEY_PREFIX: &str = "ai-mcp-auth-";

/// 描述一个由用户明确配置、通过 stdio 或远程 HTTP 连接的 MCP 工具服务器。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub auth_configured: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub allow_write: bool,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

/// 创建或更新 MCP 服务器；stdio 命令不会经过 shell，远程认证令牌只进入系统密钥链。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMcpServerInput {
    pub id: Option<String>,
    pub name: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    pub auth_token: Option<SecretString>,
    #[serde(default)]
    pub clear_auth_token: bool,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub allow_write: bool,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
}

/// 删除一个本地 MCP 服务器配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteMcpServerInput {
    pub id: String,
}

/// 返回 MCP 工具的有限元数据，不包含工具调用结果或服务器进程输出。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolSummary {
    pub server_id: String,
    pub server_name: String,
    pub name: String,
    pub description: String,
    pub read_only: bool,
}

/// 返回一次 MCP 工具发现结果及探测时间。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpProbeResult {
    pub server_id: String,
    pub tools: Vec<McpToolSummary>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct DiscoveredTool {
    server_id: String,
    server_name: String,
    name: String,
    description: String,
    input_schema: Value,
    read_only: bool,
}

#[derive(Debug, Clone)]
struct ToolBinding {
    function_name: String,
    server_id: String,
    tool_name: String,
}

struct McpClient {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    timeout: Duration,
    next_id: u64,
}

/// 保存一个远程 MCP HTTP 客户端的会话、认证令牌和 JSON-RPC 请求状态。
struct HttpMcpClient {
    client: reqwest::Client,
    url: reqwest::Url,
    auth_token: Option<SecretString>,
    timeout: Duration,
    next_id: u64,
    session_id: Option<String>,
}

/// 统一 stdio 与远程 HTTP MCP 连接，让智能体调用逻辑不感知传输层。
enum McpConnection {
    Stdio(Box<McpClient>),
    Http(HttpMcpClient),
}

/// 保存当前智能体请求期间的 MCP 子进程、工具绑定和 OpenAI function schema。
pub struct Runtime {
    clients: HashMap<String, McpConnection>,
    tools: Vec<DiscoveredTool>,
    bindings: Vec<ToolBinding>,
}

impl Runtime {
    /// 启动全部启用的 MCP 服务器，完成 initialize 和 tools/list 握手。
    pub async fn load(
        local: &LocalRepository,
        credentials: &Arc<dyn CredentialStore>,
    ) -> AppResult<Self> {
        let configs = list(local, credentials).await?;
        let mut clients = HashMap::new();
        let mut tools = Vec::new();
        for config in configs.into_iter().filter(|config| config.enabled) {
            let (client, discovered) = connect(&config, credentials).await?;
            clients.insert(config.id.clone(), client);
            tools.extend(
                discovered
                    .into_iter()
                    .filter(|tool| config.allow_write || tool.read_only),
            );
        }
        let bindings = build_bindings(&tools);
        Ok(Self {
            clients,
            tools,
            bindings,
        })
    }

    /// 将只读或用户明确允许写入的 MCP 工具转换为 OpenAI function 定义。
    pub fn openai_tools(&self) -> Value {
        Value::Array(
            self.tools
                .iter()
                .zip(self.bindings.iter())
                .map(|(tool, binding)| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": binding.function_name,
                            "description": format!("[{}] {}", tool.server_name, tool.description),
                            "parameters": tool.input_schema,
                        }
                    })
                })
                .collect(),
        )
    }

    /// 执行模型选中的 MCP 工具，并把结果限制为有界 JSON 文本。
    pub async fn call(&mut self, function_name: &str, arguments: &str) -> String {
        let Some(binding) = self
            .bindings
            .iter()
            .find(|binding| binding.function_name == function_name)
            .cloned()
        else {
            return json!({ "error": "MCP 工具不存在或未获允许" }).to_string();
        };
        let Ok(arguments) = serde_json::from_str::<Value>(arguments) else {
            return json!({ "error": "MCP 工具参数不是有效 JSON" }).to_string();
        };
        if !arguments.is_object() {
            return json!({ "error": "MCP 工具参数必须是 JSON 对象" }).to_string();
        }
        let Some(client) = self.clients.get_mut(&binding.server_id) else {
            return json!({ "error": "MCP 服务器进程已关闭" }).to_string();
        };
        let result = match client {
            McpConnection::Stdio(client) => {
                client
                    .request(
                        "tools/call",
                        json!({ "name": binding.tool_name, "arguments": arguments }),
                    )
                    .await
            }
            McpConnection::Http(client) => {
                client
                    .request(
                        "tools/call",
                        json!({ "name": binding.tool_name, "arguments": arguments }),
                    )
                    .await
            }
        };
        match result {
            Ok(value) => bounded_json(&value),
            Err(error) => json!({ "error": error.message }).to_string(),
        }
    }
}

/// 按配置选择 stdio 或 HTTP 传输，并执行 MCP 初始化和工具发现握手。
async fn connect(
    config: &McpServerConfig,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<(McpConnection, Vec<DiscoveredTool>)> {
    match config.transport.as_str() {
        "stdio" => {
            let (client, tools) = McpClient::connect(config).await?;
            Ok((McpConnection::Stdio(Box::new(client)), tools))
        }
        "http" => {
            let auth_token = credentials.get(&auth_key(&config.id)).ok();
            let (client, tools) = HttpMcpClient::connect(config, auth_token).await?;
            Ok((McpConnection::Http(client), tools))
        }
        _ => Err(AppError::new("VALIDATION_FAILED", "ai", "MCP 传输类型无效")),
    }
}

impl McpClient {
    /// 启动单个 MCP 服务器并完成协议初始化和工具发现。
    async fn connect(config: &McpServerConfig) -> AppResult<(Self, Vec<DiscoveredTool>)> {
        let mut child = Command::new(&config.command)
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                AppError::new("MCP_START_FAILED", "ai", "无法启动 MCP 服务器").details(error)
            })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::new("MCP_START_FAILED", "ai", "MCP 服务器未提供 stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::new("MCP_START_FAILED", "ai", "MCP 服务器未提供 stdout"))?;
        let mut client = Self {
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            timeout: Duration::from_secs(config.timeout_seconds.clamp(2, 60)),
            next_id: 1,
        };
        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "1panel-client", "version": "0.1.0" }
                }),
            )
            .await?;
        client
            .notification("notifications/initialized", json!({}))
            .await?;
        let response = client.request("tools/list", json!({})).await?;
        let tools = parse_tools(config, &response)?;
        Ok((client, tools))
    }

    /// 发送带数字 ID 的 JSON-RPC 请求并等待匹配响应，忽略服务器通知。
    async fn request(&mut self, method: &str, params: Value) -> AppResult<Value> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.write_message(&request).await?;
        loop {
            let mut line = String::new();
            let read = timeout(self.timeout, self.stdout.read_line(&mut line))
                .await
                .map_err(|_| AppError::new("MCP_TIMEOUT", "ai", "MCP 服务器响应超时"))?
                .map_err(|error| {
                    AppError::new("MCP_READ_FAILED", "ai", "读取 MCP 响应失败").details(error)
                })?;
            if read == 0 {
                return Err(AppError::new("MCP_CLOSED", "ai", "MCP 服务器已关闭"));
            }
            let message = serde_json::from_str::<Value>(line.trim()).map_err(|error| {
                AppError::new("MCP_RESPONSE_INVALID", "ai", "MCP 返回了无效 JSON").details(error)
            })?;
            if message.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = message.get("error") {
                return Err(AppError::new("MCP_REQUEST_FAILED", "ai", "MCP 请求失败")
                    .details(crate::security::redact(&error.to_string())));
            }
            return Ok(message.get("result").cloned().unwrap_or_else(|| json!({})));
        }
    }

    /// 发送无需响应的 JSON-RPC 通知，并确保消息立即写入子进程。
    async fn notification(&mut self, method: &str, params: Value) -> AppResult<()> {
        self.write_message(&json!({ "jsonrpc": "2.0", "method": method, "params": params }))
            .await
    }

    /// 以单行 JSON 写入 MCP stdin，不经过 shell 或额外编码层。
    async fn write_message(&mut self, message: &Value) -> AppResult<()> {
        let payload = serde_json::to_string(message).map_err(AppError::database)?;
        self.stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(|error| {
                AppError::new("MCP_WRITE_FAILED", "ai", "写入 MCP 请求失败").details(error)
            })?;
        self.stdin.write_all(b"\n").await.map_err(|error| {
            AppError::new("MCP_WRITE_FAILED", "ai", "写入 MCP 请求失败").details(error)
        })?;
        self.stdin.flush().await.map_err(|error| {
            AppError::new("MCP_WRITE_FAILED", "ai", "刷新 MCP 请求失败").details(error)
        })
    }
}

impl HttpMcpClient {
    /// 创建远程 MCP HTTP 客户端并完成 initialize/tools/list 握手。
    async fn connect(
        config: &McpServerConfig,
        auth_token: Option<SecretString>,
    ) -> AppResult<(Self, Vec<DiscoveredTool>)> {
        let url = validate_url(config.url.as_deref().unwrap_or_default())?;
        let timeout = Duration::from_secs(config.timeout_seconds.clamp(2, 60));
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| {
                AppError::new("MCP_NETWORK_FAILED", "ai", "无法初始化远程 MCP 网络客户端")
                    .details(error)
            })?;
        let mut client = Self {
            client,
            url,
            auth_token,
            timeout,
            next_id: 1,
            session_id: None,
        };
        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "1panel-client", "version": "0.1.0" }
                }),
            )
            .await?;
        client
            .notification("notifications/initialized", json!({}))
            .await?;
        let response = client.request("tools/list", json!({})).await?;
        let tools = parse_tools(config, &response)?;
        Ok((client, tools))
    }

    /// 发送带数字 ID 的远程 JSON-RPC 请求，并兼容 JSON 与 SSE 响应。
    async fn request(&mut self, method: &str, params: Value) -> AppResult<Value> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let request = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        let response = self.post(&request).await?.ok_or_else(|| {
            AppError::new(
                "MCP_RESPONSE_INVALID",
                "ai",
                "远程 MCP 未返回 JSON-RPC 响应",
            )
        })?;
        if response.get("id").and_then(Value::as_u64) != Some(id) {
            return Err(AppError::new(
                "MCP_RESPONSE_INVALID",
                "ai",
                "远程 MCP 返回了不匹配的请求 ID",
            ));
        }
        if let Some(error) = response.get("error") {
            return Err(
                AppError::new("MCP_REQUEST_FAILED", "ai", "远程 MCP 请求失败")
                    .details(crate::security::redact(&error.to_string())),
            );
        }
        Ok(response.get("result").cloned().unwrap_or_else(|| json!({})))
    }

    /// 发送无需响应的 MCP 通知；服务器返回空 body 或 2xx JSON 都视为成功。
    async fn notification(&mut self, method: &str, params: Value) -> AppResult<()> {
        let request = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.post(&request).await.map(|_| ())
    }

    /// 写入一个远程 HTTP JSON-RPC 请求并记录服务器返回的 MCP 会话 ID。
    async fn post(&mut self, request: &Value) -> AppResult<Option<Value>> {
        let mut builder = self
            .client
            .post(self.url.clone())
            .header(
                reqwest::header::ACCEPT,
                "application/json, text/event-stream",
            )
            .json(request);
        if let Some(token) = self.auth_token.as_ref() {
            builder = builder.bearer_auth(token.expose_secret());
        }
        if let Some(session_id) = self.session_id.as_deref() {
            builder = builder.header("Mcp-Session-Id", session_id);
        }
        let response = timeout(self.timeout, builder.send())
            .await
            .map_err(|_| AppError::new("MCP_TIMEOUT", "ai", "远程 MCP 响应超时"))?
            .map_err(|error| {
                AppError::new("MCP_NETWORK_FAILED", "ai", "远程 MCP 请求失败").details(error)
            })?;
        if let Some(value) = response.headers().get("Mcp-Session-Id") {
            if let Ok(value) = value.to_str() {
                if !value.is_empty() && value.len() <= 256 && !has_control(value) {
                    self.session_id = Some(value.to_string());
                }
            }
        }
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let payload = response.bytes().await.map_err(|error| {
            AppError::new("MCP_NETWORK_FAILED", "ai", "读取远程 MCP 响应失败").details(error)
        })?;
        if !status.is_success() {
            return Err(AppError::new(
                "MCP_REQUEST_FAILED",
                "ai",
                format!("远程 MCP 服务返回 HTTP {}", status.as_u16()),
            )
            .details(crate::security::redact(&String::from_utf8_lossy(&payload))));
        }
        parse_http_response(&payload, &content_type)
    }
}

impl Drop for McpClient {
    /// 释放 MCP 运行时资源时尽力终止子进程，避免后台进程脱离客户端。
    fn drop(&mut self) {
        let _ = self.child.start_kill();
    }
}

/// 读取本地 MCP 配置，并仅以布尔值回传远程认证令牌是否存在。
pub async fn list(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<Vec<McpServerConfig>> {
    let value = local.get_setting(SETTINGS_KEY).await?;
    let mut servers = value
        .map(|value| {
            serde_json::from_str::<Vec<McpServerConfig>>(&value).map_err(|error| {
                AppError::new("MCP_CONFIG_INVALID", "ai", "MCP 配置无法解析").details(error)
            })
        })
        .transpose()
        .map(|values| values.unwrap_or_default())?;
    for server in &mut servers {
        server.auth_configured =
            server.transport == "http" && credentials.get(&auth_key(&server.id)).is_ok();
    }
    Ok(servers)
}

/// 保存 MCP 配置；远程令牌写入系统密钥链，命令以直接进程方式执行。
pub async fn save(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: SaveMcpServerInput,
) -> AppResult<McpServerConfig> {
    validate_input(&input)?;
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if !valid_id(&id) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "MCP 服务器 ID 无效",
        ));
    }
    let transport = input.transport.trim().to_ascii_lowercase();
    let auth_key = auth_key(&id);
    if let Some(token) = input.auth_token.as_ref() {
        credentials.put(
            &auth_key,
            SecretString::from(token.expose_secret().trim().to_owned()),
        )?;
    } else if input.clear_auth_token {
        credentials.delete(&auth_key)?;
    }
    let auth_configured = transport == "http" && credentials.get(&auth_key).is_ok();
    let mut servers = list(local, credentials).await?;
    let config = McpServerConfig {
        id: id.clone(),
        name: input.name.trim().to_string(),
        transport,
        command: input.command.trim().to_string(),
        args: input.args,
        url: input
            .url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        auth_configured,
        enabled: input.enabled,
        allow_write: input.allow_write,
        timeout_seconds: input.timeout_seconds.clamp(2, 60),
    };
    if let Some(index) = servers.iter().position(|server| server.id == id) {
        servers[index] = config.clone();
    } else {
        if servers.len() >= MAX_SERVERS {
            return Err(AppError::new(
                "MCP_LIMIT_REACHED",
                "ai",
                "MCP 服务器数量已达到上限",
            ));
        }
        servers.push(config.clone());
    }
    persist(local, &servers).await?;
    Ok(config)
}

/// 删除 MCP 配置；已退出的运行时子进程会随本次 AI 请求结束自动清理。
pub async fn delete(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: DeleteMcpServerInput,
) -> AppResult<()> {
    if !valid_id(&input.id) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "MCP 服务器 ID 无效",
        ));
    }
    let mut servers = list(local, credentials).await?;
    servers.retain(|server| server.id != input.id);
    credentials.delete(&auth_key(&input.id))?;
    persist(local, &servers).await
}

/// 启动指定 MCP 服务器并执行真实 tools/list 探测，不会调用任何 AI 模型。
pub async fn probe(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    server_id: &str,
) -> AppResult<McpProbeResult> {
    if !valid_id(server_id) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "MCP 服务器 ID 无效",
        ));
    }
    let config = list(local, credentials)
        .await?
        .into_iter()
        .find(|server| server.id == server_id)
        .ok_or_else(|| AppError::new("MCP_NOT_FOUND", "ai", "MCP 服务器不存在"))?;
    let (client, tools) = connect(&config, credentials).await?;
    drop(client);
    Ok(McpProbeResult {
        server_id: config.id,
        tools: tools
            .into_iter()
            .map(|tool| McpToolSummary {
                server_id: tool.server_id,
                server_name: tool.server_name,
                name: tool.name,
                description: tool.description,
                read_only: tool.read_only,
            })
            .collect(),
        checked_at: Utc::now(),
    })
}

/// 解析远程 MCP 的 JSON 或 text/event-stream 响应，忽略 SSE 注释和事件名行。
fn parse_http_response(payload: &[u8], _content_type: &str) -> AppResult<Option<Value>> {
    if payload.is_empty() {
        return Ok(None);
    }
    if let Ok(value) = serde_json::from_slice::<Value>(payload) {
        return Ok(Some(value));
    }
    let text = String::from_utf8_lossy(payload);
    for line in text.lines() {
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(data) {
            return Ok(Some(value));
        }
    }
    Err(AppError::new(
        "MCP_RESPONSE_INVALID",
        "ai",
        "远程 MCP 返回了无效 JSON 或 SSE",
    ))
}

/// 验证远程 MCP URL，拒绝凭据、片段、控制字符和非 HTTP(S) 协议。
fn validate_url(value: &str) -> AppResult<reqwest::Url> {
    if value.is_empty() || value.len() > MAX_URL_LENGTH || has_control(value) {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "MCP URL 无效"));
    }
    let url = reqwest::Url::parse(value)
        .map_err(|_| AppError::new("VALIDATION_FAILED", "ai", "MCP URL 无效"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "MCP URL 必须是无凭据 HTTP(S) 地址",
        ));
    }
    Ok(url)
}

/// 校验 MCP 配置，并根据传输类型要求命令或远端 URL。
fn validate_input(input: &SaveMcpServerInput) -> AppResult<()> {
    if input.name.trim().is_empty() || input.name.chars().count() > 120 || has_control(&input.name)
    {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "MCP 名称无效"));
    }
    let transport = input.transport.trim().to_ascii_lowercase();
    if !matches!(transport.as_str(), "stdio" | "http") {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "MCP 传输类型无效"));
    }
    if transport == "stdio"
        && (input.command.trim().is_empty()
            || input.command.len() > 512
            || has_control(&input.command))
    {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "MCP 启动命令无效"));
    }
    if transport == "http" {
        validate_url(input.url.as_deref().unwrap_or_default())?;
    }
    if input.args.len() > MAX_ARGS
        || input
            .args
            .iter()
            .any(|value| value.len() > 2048 || has_control(value))
    {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "MCP 启动参数无效"));
    }
    if transport == "http" && !input.args.is_empty() {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "HTTP MCP 不支持启动参数",
        ));
    }
    if let Some(token) = input.auth_token.as_ref() {
        let value = token.expose_secret().trim();
        if value.is_empty() || value.len() > MAX_AUTH_TOKEN_LENGTH || has_control(value) {
            return Err(AppError::new("VALIDATION_FAILED", "ai", "MCP 认证令牌无效"));
        }
    }
    if !(2..=60).contains(&input.timeout_seconds) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "MCP 超时时间必须在 2-60 秒",
        ));
    }
    Ok(())
}

/// 解析 MCP tools/list 结果，限制工具数量和字段长度以保护模型上下文。
fn parse_tools(config: &McpServerConfig, response: &Value) -> AppResult<Vec<DiscoveredTool>> {
    let Some(values) = response.get("tools").and_then(Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut tools = Vec::new();
    for value in values.iter().take(MAX_TOOLS_PER_SERVER) {
        let Some(name) = value.get("name").and_then(Value::as_str) else {
            continue;
        };
        if name.trim().is_empty() || name.len() > 160 || has_control(name) {
            continue;
        }
        let description = value
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("MCP 工具")
            .chars()
            .take(1000)
            .collect::<String>();
        let input_schema = value
            .get("inputSchema")
            .filter(|schema| schema.is_object())
            .cloned()
            .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
        let read_only = value
            .get("annotations")
            .and_then(|annotations| {
                annotations
                    .get("readOnlyHint")
                    .or_else(|| annotations.get("readOnly"))
            })
            .and_then(Value::as_bool)
            .unwrap_or(false);
        tools.push(DiscoveredTool {
            server_id: config.id.clone(),
            server_name: config.name.clone(),
            name: name.to_string(),
            description,
            input_schema,
            read_only,
        });
    }
    Ok(tools)
}

/// 为 MCP 工具生成稳定且符合 OpenAI function name 约束的命名空间。
fn build_bindings(tools: &[DiscoveredTool]) -> Vec<ToolBinding> {
    let mut used = HashSet::new();
    tools
        .iter()
        .enumerate()
        .map(|(index, tool)| {
            let base = sanitize_function_name(&format!("mcp_{}_{}", tool.server_id, tool.name));
            let mut function_name = base.chars().take(64).collect::<String>();
            if !used.insert(function_name.clone()) {
                let suffix = format!("_{index}");
                let prefix_len = 64_usize.saturating_sub(suffix.len());
                function_name = format!(
                    "{}{}",
                    base.chars().take(prefix_len).collect::<String>(),
                    suffix
                );
                used.insert(function_name.clone());
            }
            ToolBinding {
                function_name,
                server_id: tool.server_id.clone(),
                tool_name: tool.name.clone(),
            }
        })
        .collect()
}

/// 限制 MCP 工具函数名为模型常见的安全字符集合。
fn sanitize_function_name(value: &str) -> String {
    let mut output = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .take(64)
        .collect::<String>();
    if output.is_empty() {
        output.push_str("mcp_tool");
    }
    output
}

/// 将 MCP 返回值序列化并截断到模型可接受的有限大小。
fn bounded_json(value: &Value) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "{}".into());
    if serialized.len() <= MAX_TOOL_RESULT_BYTES {
        serialized
    } else {
        let mut output = serialized
            .chars()
            .take(MAX_TOOL_RESULT_BYTES.saturating_sub(64))
            .collect::<String>();
        output.push_str("…[结果已截断]");
        output
    }
}

/// 判断字符串是否包含会破坏 JSON-RPC 行协议的控制字符。
fn has_control(value: &str) -> bool {
    value.chars().any(char::is_control)
}

/// 判断本地 ID 是否可安全用于设置和 function 命名空间。
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// 生成远程 MCP 认证令牌在系统密钥链中的稳定引用。
fn auth_key(server_id: &str) -> String {
    format!("{AUTH_KEY_PREFIX}{server_id}")
}

/// 将 MCP 配置序列化保存到本地 SQLite 设置，不包含 API key 或远端工具结果。
async fn persist(local: &LocalRepository, servers: &[McpServerConfig]) -> AppResult<()> {
    if servers.is_empty() {
        return local.delete_setting(SETTINGS_KEY).await;
    }
    let json = serde_json::to_string(servers).map_err(AppError::database)?;
    local.set_setting(SETTINGS_KEY, &json).await
}

/// 为缺少 enabled 字段的旧 MCP 配置提供默认启用状态。
fn default_enabled() -> bool {
    true
}

/// 为缺少 timeout 字段的旧 MCP 配置提供有限默认值。
fn default_timeout_seconds() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

/// 为缺少 transport 字段的旧 MCP 配置提供 stdio 兼容默认值。
fn default_transport() -> String {
    "stdio".into()
}

#[cfg(test)]
mod tests {
    use super::{
        build_bindings, parse_http_response, parse_tools, sanitize_function_name, validate_input,
        validate_url, McpServerConfig, SaveMcpServerInput,
    };
    use secrecy::SecretString;
    use serde_json::json;

    /// 构造测试用 MCP 配置，避免测试依赖本机外部命令。
    fn config() -> McpServerConfig {
        McpServerConfig {
            id: "server-1".into(),
            name: "Demo MCP".into(),
            transport: "stdio".into(),
            command: "demo-mcp".into(),
            args: Vec::new(),
            url: None,
            auth_configured: false,
            enabled: true,
            allow_write: false,
            timeout_seconds: 15,
        }
    }

    #[test]
    fn parses_read_only_tool_metadata() {
        let tools = parse_tools(
            &config(),
            &json!({
                "tools": [
                    {"name":"status.read","description":"Read status","inputSchema":{"type":"object"},"annotations":{"readOnlyHint":true}},
                    {"name":"restart","description":"Restart service","inputSchema":{"type":"object"}}
                ]
            }),
        )
        .unwrap();
        assert_eq!(tools.len(), 2);
        assert!(tools[0].read_only);
        assert!(!tools[1].read_only);
        let bindings = build_bindings(&tools);
        assert!(bindings[0]
            .function_name
            .starts_with("mcp_server-1_status_read"));
    }

    #[test]
    fn sanitizes_and_bounds_function_names() {
        let value = sanitize_function_name("mcp/危险 tool");
        assert_eq!(value, "mcp____tool");
        assert_eq!(sanitize_function_name(&"a".repeat(200)).len(), 64);
    }

    /// 构造远程 MCP 保存输入，覆盖 URL、令牌和默认策略校验。
    fn http_input(url: &str) -> SaveMcpServerInput {
        SaveMcpServerInput {
            id: None,
            name: "Remote MCP".into(),
            transport: "http".into(),
            command: String::new(),
            args: Vec::new(),
            url: Some(url.into()),
            auth_token: Some(SecretString::from("token-value")),
            clear_auth_token: false,
            enabled: true,
            allow_write: false,
            timeout_seconds: 15,
        }
    }

    #[test]
    fn parses_json_and_sse_http_responses() {
        assert_eq!(
            parse_http_response(
                br#"{"jsonrpc":"2.0","id":1,"result":{}}"#,
                "application/json"
            )
            .unwrap()
            .unwrap()["id"],
            1
        );
        assert_eq!(
            parse_http_response(
                b"event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2}\n\n",
                "text/event-stream"
            )
            .unwrap()
            .unwrap()["id"],
            2
        );
    }

    #[test]
    fn validates_remote_url_and_auth_token() {
        validate_input(&http_input("https://mcp.example.com/mcp")).unwrap();
        assert!(validate_url("https://user:pass@mcp.example.com/mcp").is_err());
        assert!(validate_url("file:///tmp/mcp").is_err());
        let mut input = http_input("https://mcp.example.com/mcp");
        input.auth_token = Some(SecretString::from("line\nfeed"));
        assert!(validate_input(&input).is_err());
    }

    #[test]
    fn deserializes_legacy_stdio_config_with_defaults() {
        let config = serde_json::from_value::<McpServerConfig>(json!({
            "id": "legacy",
            "name": "Legacy",
            "command": "demo"
        }))
        .unwrap();
        assert_eq!(config.transport, "stdio");
        assert!(!config.auth_configured);
    }
}
