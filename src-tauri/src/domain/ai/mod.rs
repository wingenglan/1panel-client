use crate::domain::ssh::SshConnectionManager;
use crate::errors::{AppError, AppResult};
use crate::infra::local::LocalRepository;
use crate::security::CredentialStore;
use chrono::{DateTime, Utc};
use reqwest::Client;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::ipc::Channel;
use uuid::Uuid;

pub mod mcp;

const SETTINGS_KEY: &str = "ai.providers";
const KEY_PREFIX: &str = "ai-api-key-";
const CONVERSATIONS_KEY: &str = "ai.conversations";
const MAX_CONVERSATIONS: usize = 50;
const MAX_HISTORY_BYTES: usize = 4 * 1024 * 1024;

/// 描述一个可用于聊天的 OpenAI-compatible 供应商，不返回 API key 明文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProvider {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub model: String,
    pub enabled: bool,
    pub has_api_key: bool,
}

/// 描述供应商 `/models` 返回的有限模型元数据，不暴露响应中的其他字段。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: String,
    pub owned_by: Option<String>,
}

/// 保存供应商配置；API key 只写入操作系统密钥链。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAiProviderInput {
    pub id: Option<String>,
    pub name: String,
    pub base_url: String,
    pub model: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub api_key: Option<SecretString>,
    #[serde(default)]
    pub clear_api_key: bool,
}

/// 删除 AI 供应商请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAiProviderInput {
    pub id: String,
}

/// 描述一条发送给模型的聊天消息。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AiChatMessage {
    pub role: String,
    pub content: String,
}

/// AI 聊天请求；消息会在发送前做长度和角色校验。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatInput {
    pub provider_id: String,
    pub messages: Vec<AiChatMessage>,
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Optional local task identifier used only by streaming chat cancellation.
    #[serde(default)]
    pub task_id: Option<String>,
}

/// 返回模型生成文本和基础 usage 信息。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiChatResult {
    pub provider_id: String,
    pub model: String,
    pub content: String,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
}

/// 描述一条本地 AI 对话历史；只包含消息和供应商引用，不包含 API key 或远端原始响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiConversation {
    pub id: String,
    pub provider_id: String,
    pub title: String,
    pub messages: Vec<AiChatMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 保存或更新本地 AI 对话历史；消息会在落盘前重新经过角色和长度校验。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAiConversationInput {
    pub id: Option<String>,
    pub provider_id: String,
    pub title: Option<String>,
    pub messages: Vec<AiChatMessage>,
}

/// 删除一条本地 AI 对话历史。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAiConversationInput {
    pub id: String,
}

/// AI 只读智能体请求；工具范围固定为当前服务器系统概览，最大步数由服务端限制。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgentInput {
    pub provider_id: String,
    pub server_id: String,
    pub messages: Vec<AiChatMessage>,
    #[serde(default)]
    pub max_steps: Option<u8>,
    /// 是否在本次只读智能体请求中启动已启用的 MCP 工具服务器。
    #[serde(default)]
    pub mcp_enabled: bool,
}

/// 返回智能体最终文本和实际执行的只读工具步数。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAgentResult {
    pub provider_id: String,
    pub model: String,
    pub content: String,
    pub steps: u8,
    pub tool_calls: u8,
}

/// 描述 OpenAI-compatible SSE 流中的增量事件；事件内容不包含 API key 或完整响应原文。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum AiStreamEvent {
    Delta {
        content: String,
    },
    Completed {
        model: String,
        prompt_tokens: Option<u64>,
        completion_tokens: Option<u64>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredProvider {
    id: String,
    name: String,
    base_url: String,
    model: String,
    enabled: bool,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelResponse>,
}

#[derive(Debug, Deserialize)]
struct ModelResponse {
    id: String,
    owned_by: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessageResponse,
}

#[derive(Debug, Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCallResponse>,
}

#[derive(Debug, Deserialize)]
struct ToolCallResponse {
    id: String,
    function: ToolFunctionResponse,
}

#[derive(Debug, Deserialize)]
struct ToolFunctionResponse {
    name: String,
    arguments: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct StreamResponse {
    choices: Vec<StreamChoice>,
    usage: Option<ChatUsage>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamChoice {
    delta: Option<StreamDelta>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    content: Option<String>,
}

/// 读取本地 AI 供应商列表，并仅暴露是否存在密钥的布尔值。
pub async fn list(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
) -> AppResult<Vec<AiProvider>> {
    let stored = load_stored(local).await?;
    Ok(stored
        .into_iter()
        .map(|provider| AiProvider {
            has_api_key: credentials.get(&key_ref(&provider.id)).is_ok(),
            id: provider.id,
            name: provider.name,
            base_url: provider.base_url,
            model: provider.model,
            enabled: provider.enabled,
        })
        .collect())
}

/// 保存一个 OpenAI-compatible 供应商配置，并将 key 与普通配置分开存储。
pub async fn save(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: SaveAiProviderInput,
) -> AppResult<AiProvider> {
    validate_provider(&input)?;
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    if !valid_id(&id) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI 供应商 ID 无效",
        ));
    }
    let mut providers = load_stored(local).await?;
    let index = providers.iter().position(|provider| provider.id == id);
    if let Some(secret) = input
        .api_key
        .as_ref()
        .filter(|value| !value.expose_secret().is_empty())
    {
        credentials.put(
            &key_ref(&id),
            SecretString::from(secret.expose_secret().to_owned()),
        )?;
    } else if input.clear_api_key {
        credentials.delete(&key_ref(&id))?;
    }
    let value = StoredProvider {
        id: id.clone(),
        name: input.name.trim().into(),
        base_url: normalize_base_url(&input.base_url),
        model: input.model.trim().into(),
        enabled: input.enabled,
    };
    if let Some(index) = index {
        providers[index] = value;
    } else {
        providers.push(value);
    }
    if providers.is_empty() {
        local.delete_setting(SETTINGS_KEY).await?;
    } else {
        let json = serde_json::to_string(&providers).map_err(AppError::database)?;
        local.set_setting(SETTINGS_KEY, &json).await?;
    }
    let current_key = if input.clear_api_key
        && input
            .api_key
            .as_ref()
            .map(|value| value.expose_secret().is_empty())
            .unwrap_or(true)
    {
        None
    } else {
        credentials.get(&key_ref(&id)).ok()
    };
    Ok(AiProvider {
        id,
        name: input.name.trim().into(),
        base_url: normalize_base_url(&input.base_url),
        model: input.model.trim().into(),
        enabled: input.enabled,
        has_api_key: current_key.is_some(),
    })
}

/// 删除供应商配置和对应的系统密钥链条目。
pub async fn delete(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: DeleteAiProviderInput,
) -> AppResult<()> {
    if !valid_id(&input.id) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI 供应商 ID 无效",
        ));
    }
    let mut providers = load_stored(local).await?;
    providers.retain(|provider| provider.id != input.id);
    if providers.is_empty() {
        local.delete_setting(SETTINGS_KEY).await?;
    } else {
        let json = serde_json::to_string(&providers).map_err(AppError::database)?;
        local.set_setting(SETTINGS_KEY, &json).await?;
    }
    credentials.delete(&key_ref(&input.id))?;
    Ok(())
}

/// 读取本机保存的 AI 对话，按最近更新时间倒序返回并按供应商筛选。
pub async fn list_conversations(
    local: &LocalRepository,
    provider_id: Option<&str>,
) -> AppResult<Vec<AiConversation>> {
    if provider_id.is_some_and(|value| !valid_id(value)) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI 供应商 ID 无效",
        ));
    }
    let mut conversations = load_conversations(local).await?;
    if let Some(provider_id) = provider_id {
        conversations.retain(|conversation| conversation.provider_id == provider_id);
    }
    conversations.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at));
    conversations.truncate(MAX_CONVERSATIONS);
    Ok(conversations)
}

/// 保存一条脱敏 AI 对话历史；只写入本地 SQLite 设置，不写入密钥链或远端服务器。
pub async fn save_conversation(
    local: &LocalRepository,
    input: SaveAiConversationInput,
) -> AppResult<AiConversation> {
    validate_conversation(&input)?;
    let mut conversations = load_conversations(local).await?;
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let now = Utc::now();
    let title = conversation_title(input.title.as_deref(), &input.messages);
    let created_at = conversations
        .iter()
        .find(|conversation| conversation.id == id)
        .map(|conversation| conversation.created_at)
        .unwrap_or(now);
    let conversation = AiConversation {
        id: id.clone(),
        provider_id: input.provider_id,
        title,
        messages: input.messages,
        created_at,
        updated_at: now,
    };
    conversations.retain(|current| current.id != id);
    conversations.push(conversation.clone());
    conversations.sort_by_key(|conversation| std::cmp::Reverse(conversation.updated_at));
    conversations.truncate(MAX_CONVERSATIONS);
    persist_conversations(local, &conversations).await?;
    Ok(conversation)
}

/// 删除一条本地 AI 对话历史；不存在的 ID 按幂等删除处理。
pub async fn delete_conversation(
    local: &LocalRepository,
    input: DeleteAiConversationInput,
) -> AppResult<()> {
    if !valid_id(&input.id) {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "AI 对话 ID 无效"));
    }
    let mut conversations = load_conversations(local).await?;
    conversations.retain(|conversation| conversation.id != input.id);
    persist_conversations(local, &conversations).await
}

/// 清理本地 AI 对话历史，可只清理当前供应商以避免影响其他模型的会话。
pub async fn clear_conversations(
    local: &LocalRepository,
    provider_id: Option<&str>,
) -> AppResult<()> {
    if provider_id.is_some_and(|value| !valid_id(value)) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI 供应商 ID 无效",
        ));
    }
    let mut conversations = load_conversations(local).await?;
    if let Some(provider_id) = provider_id {
        conversations.retain(|conversation| conversation.provider_id != provider_id);
    } else {
        conversations.clear();
    }
    persist_conversations(local, &conversations).await
}

/// 读取真实 OpenAI-compatible `/models` 列表，仅返回有限的模型 ID 和归属字段。
pub async fn models(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    provider_id: &str,
) -> AppResult<Vec<AiModel>> {
    if !valid_id(provider_id) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI 供应商 ID 无效",
        ));
    }
    let provider = load_stored(local)
        .await?
        .into_iter()
        .find(|provider| provider.id == provider_id && provider.enabled)
        .ok_or_else(|| AppError::new("AI_PROVIDER_NOT_FOUND", "ai", "AI 供应商不存在或已禁用"))?;
    let key = credentials.get(&key_ref(&provider.id)).ok();
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| {
            AppError::new("AI_NETWORK_FAILED", "ai", "无法初始化 AI 网络客户端").details(error)
        })?;
    let mut request = client
        .get(models_endpoint(&provider.base_url))
        .header("User-Agent", "1Panel-client");
    if let Some(key) = key {
        request = request.bearer_auth(key.expose_secret());
    }
    let response = request.send().await.map_err(|error| {
        AppError::new("AI_NETWORK_FAILED", "ai", "AI 模型列表请求失败").details(error)
    })?;
    let status = response.status();
    let payload = response.text().await.map_err(|error| {
        AppError::new("AI_NETWORK_FAILED", "ai", "AI 模型列表读取失败").details(error)
    })?;
    if !status.is_success() {
        return Err(AppError::new(
            "AI_REQUEST_FAILED",
            "ai",
            format!("AI 服务返回 HTTP {}", status.as_u16()),
        )
        .details(crate::security::redact(&payload)));
    }
    let parsed = serde_json::from_str::<ModelsResponse>(&payload).map_err(|error| {
        AppError::new("AI_RESPONSE_INVALID", "ai", "AI 模型列表格式无效").details(error)
    })?;
    Ok(parsed
        .data
        .into_iter()
        .filter(|model| !model.id.trim().is_empty() && model.id.len() <= 256)
        .take(200)
        .map(|model| AiModel {
            id: model.id,
            owned_by: model.owned_by.filter(|value| value.len() <= 128),
        })
        .collect())
}

/// 向选定供应商发送 OpenAI-compatible chat completion 请求，API key 永不进入返回值或日志。
pub async fn chat(
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: AiChatInput,
) -> AppResult<AiChatResult> {
    validate_chat(&input)?;
    let provider = load_stored(local)
        .await?
        .into_iter()
        .find(|provider| provider.id == input.provider_id && provider.enabled)
        .ok_or_else(|| AppError::new("AI_PROVIDER_NOT_FOUND", "ai", "AI 供应商不存在或已禁用"))?;
    let key = credentials.get(&key_ref(&provider.id)).ok();
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| {
            AppError::new("AI_NETWORK_FAILED", "ai", "无法初始化 AI 网络客户端").details(error)
        })?;
    let endpoint = chat_endpoint(&provider.base_url);
    let body = serde_json::json!({
        "model": provider.model,
        "messages": input.messages,
        "temperature": input.temperature.unwrap_or(0.2).clamp(0.0, 2.0),
        "stream": false,
    });
    let mut request = client
        .post(endpoint)
        .json(&body)
        .header("User-Agent", "1panel-client");
    if let Some(key) = key {
        request = request.bearer_auth(key.expose_secret());
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::new("AI_NETWORK_FAILED", "ai", "AI 请求失败").details(error))?;
    let status = response.status();
    let payload = response.text().await.map_err(|error| {
        AppError::new("AI_NETWORK_FAILED", "ai", "AI 响应读取失败").details(error)
    })?;
    if !status.is_success() {
        return Err(AppError::new(
            "AI_REQUEST_FAILED",
            "ai",
            format!("AI 服务返回 HTTP {}", status.as_u16()),
        )
        .details(crate::security::redact(&payload)));
    }
    let parsed = serde_json::from_str::<ChatResponse>(&payload).map_err(|error| {
        AppError::new("AI_RESPONSE_INVALID", "ai", "AI 响应格式无效").details(error)
    })?;
    let content = parsed
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_deref())
        .map(str::trim)
        .map(str::to_string)
        .filter(|content| !content.is_empty())
        .ok_or_else(|| AppError::new("AI_RESPONSE_EMPTY", "ai", "AI 没有返回文本"))?;
    Ok(AiChatResult {
        provider_id: provider.id,
        model: parsed.model.unwrap_or(provider.model),
        content,
        prompt_tokens: parsed.usage.as_ref().and_then(|usage| usage.prompt_tokens),
        completion_tokens: parsed.usage.and_then(|usage| usage.completion_tokens),
    })
}

/// Runs a bounded OpenAI-compatible function-calling loop with the built-in overview tool and optional MCP tools.
pub async fn agent(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: AiAgentInput,
) -> AppResult<AiAgentResult> {
    validate_agent(&input)?;
    let provider = load_stored(local)
        .await?
        .into_iter()
        .find(|provider| provider.id == input.provider_id && provider.enabled)
        .ok_or_else(|| AppError::new("AI_PROVIDER_NOT_FOUND", "ai", "AI 供应商不存在或已禁用"))?;
    let key = credentials.get(&key_ref(&provider.id)).ok();
    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| {
            AppError::new("AI_NETWORK_FAILED", "ai", "无法初始化 AI 网络客户端").details(error)
        })?;
    let mut mcp_runtime = if input.mcp_enabled {
        Some(mcp::Runtime::load(local, credentials).await?)
    } else {
        None
    };
    let tools = agent_tools(mcp_runtime.as_ref());
    let endpoint = chat_endpoint(&provider.base_url);
    let mut messages = input
        .messages
        .iter()
        .map(|message| serde_json::json!({"role": message.role, "content": message.content}))
        .collect::<Vec<_>>();
    let max_steps = input.max_steps.unwrap_or(4).clamp(1, 6);
    let mut tool_calls = 0_u8;
    for step in 1..=max_steps {
        let body = serde_json::json!({
            "model": provider.model,
            "messages": messages,
            "temperature": 0.2,
            "stream": false,
            "tools": tools.clone(),
            "tool_choice": "auto",
        });
        let mut request = client
            .post(&endpoint)
            .json(&body)
            .header("User-Agent", "1Panel-client");
        if let Some(key) = key.as_ref() {
            request = request.bearer_auth(key.expose_secret());
        }
        let response = request.send().await.map_err(|error| {
            AppError::new("AI_NETWORK_FAILED", "ai", "AI 请求失败").details(error)
        })?;
        let status = response.status();
        let payload = response.text().await.map_err(|error| {
            AppError::new("AI_NETWORK_FAILED", "ai", "AI 响应读取失败").details(error)
        })?;
        if !status.is_success() {
            return Err(AppError::new(
                "AI_REQUEST_FAILED",
                "ai",
                format!("AI 服务返回 HTTP {}", status.as_u16()),
            )
            .details(crate::security::redact(&payload)));
        }
        let parsed = serde_json::from_str::<ChatResponse>(&payload).map_err(|error| {
            AppError::new("AI_RESPONSE_INVALID", "ai", "AI 响应格式无效").details(error)
        })?;
        let choice = parsed
            .choices
            .first()
            .ok_or_else(|| AppError::new("AI_RESPONSE_EMPTY", "ai", "AI 没有返回选择"))?;
        let message = &choice.message;
        if message.tool_calls.is_empty() {
            let content = message
                .content
                .as_deref()
                .map(str::trim)
                .filter(|content| !content.is_empty())
                .ok_or_else(|| AppError::new("AI_RESPONSE_EMPTY", "ai", "AI 智能体没有返回文本"))?;
            return Ok(AiAgentResult {
                provider_id: provider.id,
                model: parsed.model.unwrap_or(provider.model),
                content: content.to_string(),
                steps: step,
                tool_calls,
            });
        }
        let serialized_calls = message
            .tool_calls
            .iter()
            .map(|call| {
                serde_json::json!({
                    "id": call.id,
                    "type": "function",
                    "function": {"name": call.function.name, "arguments": call.function.arguments}
                })
            })
            .collect::<Vec<_>>();
        messages.push(serde_json::json!({
            "role": "assistant",
            "content": message.content.clone().unwrap_or_default(),
            "tool_calls": serialized_calls,
        }));
        for call in &message.tool_calls {
            tool_calls = tool_calls.saturating_add(1);
            let output =
                execute_agent_tool(ssh, &input.server_id, call, mcp_runtime.as_mut()).await;
            messages.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": call.id,
                "content": output,
            }));
        }
    }
    Err(AppError::new(
        "AI_AGENT_STEPS_EXCEEDED",
        "ai",
        "AI 智能体达到最大工具步数，未返回最终答案",
    ))
}

/// Declares bounded read-only server functions and any explicitly enabled MCP functions.
fn agent_tools(mcp_runtime: Option<&mcp::Runtime>) -> serde_json::Value {
    let mut tools = vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "server_overview",
                "description": "读取当前选定服务器的系统概览（CPU、内存、磁盘、运行时和监听端口数量）",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "server_websites",
                "description": "读取选定服务器的客户端受控网站、证书摘要和 PHP-FPM 能力（只读）",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "server_docker",
                "description": "读取选定服务器 Docker 版本、资源数量、容器和 Compose 项目摘要（只读）",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "server_security",
                "description": "读取选定服务器防火墙和 SSH 有效配置摘要，不返回凭据（只读）",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "server_processes",
                "description": "读取选定服务器运行中的进程与监听端口摘要，含 CPU/内存占用（只读）",
                "parameters": {"type": "object", "properties": {}, "additionalProperties": false}
            }
        }),
    ];
    if let Some(runtime) = mcp_runtime {
        if let serde_json::Value::Array(values) = runtime.openai_tools() {
            tools.extend(values);
        }
    }
    serde_json::Value::Array(tools)
}

/// Executes an allow-listed built-in or MCP tool and returns a compact JSON result for the model.
async fn execute_agent_tool(
    ssh: &SshConnectionManager,
    server_id: &str,
    call: &ToolCallResponse,
    mcp_runtime: Option<&mut mcp::Runtime>,
) -> String {
    if !matches!(
        call.function.name.as_str(),
        "server_overview"
            | "server_websites"
            | "server_docker"
            | "server_security"
            | "server_processes"
    ) {
        if let Some(runtime) = mcp_runtime {
            return runtime
                .call(&call.function.name, &call.function.arguments)
                .await;
        }
        return serde_json::json!({"error": "不允许的智能体工具"}).to_string();
    }
    if serde_json::from_str::<serde_json::Value>(&call.function.arguments).is_err() {
        return serde_json::json!({"error": "工具参数不是有效 JSON"}).to_string();
    }
    match call.function.name.as_str() {
        "server_overview" => match crate::domain::metrics::probe(ssh, server_id).await {
            Ok(overview) => serde_json::json!({
                "hostname": overview.hostname,
                "os": overview.os_name,
                "kernel": overview.kernel,
                "cpuUsagePercent": overview.cpu_usage_percent,
                "memoryTotalBytes": overview.memory_total_bytes,
                "memoryAvailableBytes": overview.memory_available_bytes,
                "load": overview.load,
                "failedServices": overview.failed_services,
                "listeningPorts": overview.listening_ports,
                "docker": overview.docker,
                "nginx": overview.nginx,
                "permissionDiagnostics": overview.permission_diagnostics,
            })
            .to_string(),
            Err(error) => serde_json::json!({"error": error.message}).to_string(),
        },
        "server_websites" => match crate::domain::website::snapshot(ssh, server_id).await {
            Ok(snapshot) => serde_json::json!({
                "supported": snapshot.supported,
                "managedConfDir": snapshot.managed_conf_dir,
                "websites": snapshot.websites.iter().take(50).collect::<Vec<_>>(),
                "counts": {
                    "websites": snapshot.websites.len(),
                    "phpRuntimes": snapshot.php_runtimes.len(),
                },
                "phpRuntimes": snapshot.php_runtimes.iter().take(30).collect::<Vec<_>>(),
                "certificateTools": snapshot.certificate_tools,
                "warnings": snapshot.warnings,
            })
            .to_string(),
            Err(error) => serde_json::json!({"error": error.message}).to_string(),
        },
        "server_docker" => match crate::domain::docker::snapshot(ssh, server_id, false).await {
            Ok(snapshot) => {
                let containers = snapshot
                    .containers
                    .iter()
                    .take(50)
                    .map(|container| {
                        serde_json::json!({
                            "name": container.name,
                            "image": container.image,
                            "status": container.status,
                            "health": container.health,
                            "ports": container.ports,
                            "composeProject": container.compose_project,
                        })
                    })
                    .collect::<Vec<_>>();
                let projects = snapshot
                    .compose_projects
                    .iter()
                    .take(50)
                    .map(|project| {
                        serde_json::json!({
                            "name": project.name,
                            "status": project.status,
                            "workingDir": project.working_dir,
                        })
                    })
                    .collect::<Vec<_>>();
                serde_json::json!({
                    "installed": snapshot.installed,
                    "running": snapshot.running,
                    "version": snapshot.version,
                    "apiVersion": snapshot.api_version,
                    "os": snapshot.os,
                    "architecture": snapshot.architecture,
                    "storageDriver": snapshot.storage_driver,
                    "rootDir": snapshot.root_dir,
                    "counts": {
                        "containers": snapshot.containers.len(),
                        "images": snapshot.images.len(),
                        "volumes": snapshot.volumes.len(),
                        "networks": snapshot.networks.len(),
                        "composeProjects": snapshot.compose_projects.len(),
                    },
                    "containers": containers,
                    "composeProjects": projects,
                })
                .to_string()
            }
            Err(error) => serde_json::json!({"error": error.message}).to_string(),
        },
        "server_security" => match crate::domain::security::snapshot(ssh, server_id).await {
            Ok(snapshot) => serde_json::json!({
                "firewall": {
                    "backend": snapshot.firewall.backend,
                    "installed": snapshot.firewall.installed,
                    "enabled": snapshot.firewall.enabled,
                    "defaultIncoming": snapshot.firewall.default_incoming,
                    "defaultOutgoing": snapshot.firewall.default_outgoing,
                    "rules": snapshot.firewall.rules.iter().take(100).collect::<Vec<_>>(),
                },
                "ssh": {
                    "configPath": snapshot.ssh.config_path,
                    "port": snapshot.ssh.port,
                    "passwordAuthentication": snapshot.ssh.password_authentication,
                    "pubkeyAuthentication": snapshot.ssh.pubkey_authentication,
                    "permitRootLogin": snapshot.ssh.permit_root_login,
                    "effectiveLines": snapshot.ssh.effective_lines.iter().take(100).collect::<Vec<_>>(),
                },
                "warnings": snapshot.warnings,
            })
            .to_string(),
            Err(error) => serde_json::json!({"error": error.message}).to_string(),
        },
        "server_processes" => match crate::domain::operations::snapshot(ssh, server_id, false).await {
            Ok(snapshot) => {
                let mut top: Vec<_> = snapshot.processes.iter().collect();
                top.sort_by(|a, b| b.cpu_percent.partial_cmp(&a.cpu_percent).unwrap_or(std::cmp::Ordering::Equal));
                let processes: Vec<_> = top.into_iter().take(60).map(|process| serde_json::json!({
                    "pid": process.pid,
                    "user": process.user,
                    "state": process.state,
                    "cpuPercent": process.cpu_percent,
                    "memoryPercent": process.memory_percent,
                    "rssBytes": process.rss_bytes,
                    "name": process.name,
                    "systemdUnit": process.systemd_unit,
                })).collect();
                let ports: Vec<_> = snapshot.ports.iter().take(80).map(|port| serde_json::json!({
                    "protocol": port.protocol,
                    "localAddress": port.local_address,
                    "port": port.port,
                    "pid": port.pid,
                    "processName": port.process_name,
                })).collect();
                serde_json::json!({
                    "processCount": snapshot.processes.len(),
                    "listeningPorts": snapshot.ports.len(),
                    "topProcesses": processes,
                    "listeningPortsDetail": ports,
                })
                .to_string()
            }
            Err(error) => serde_json::json!({"error": error.message}).to_string(),
        },
        _ => serde_json::json!({"error": "不允许的智能体工具"}).to_string(),
    }
}

/// 向 OpenAI-compatible 供应商发起 SSE 流式聊天，并通过 Tauri Channel 推送增量文本。
pub async fn stream_chat(
    ssh: &SshConnectionManager,
    local: &LocalRepository,
    credentials: &Arc<dyn CredentialStore>,
    input: AiChatInput,
    events: &Channel<AiStreamEvent>,
) -> AppResult<AiChatResult> {
    validate_chat(&input)?;
    let task_guard = input
        .task_id
        .as_deref()
        .map(|task_id| ssh.begin_command_task(task_id))
        .transpose()?;
    let provider = load_stored(local)
        .await?
        .into_iter()
        .find(|provider| provider.id == input.provider_id && provider.enabled)
        .ok_or_else(|| AppError::new("AI_PROVIDER_NOT_FOUND", "ai", "AI 供应商不存在或已禁用"))?;
    let key = credentials.get(&key_ref(&provider.id)).ok();
    let client = Client::builder()
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| {
            AppError::new("AI_NETWORK_FAILED", "ai", "无法初始化 AI 网络客户端").details(error)
        })?;
    let endpoint = chat_endpoint(&provider.base_url);
    let body = serde_json::json!({
        "model": provider.model,
        "messages": input.messages,
        "temperature": input.temperature.unwrap_or(0.2).clamp(0.0, 2.0),
        "stream": true,
        "stream_options": { "include_usage": true },
    });
    let mut request = client
        .post(endpoint)
        .json(&body)
        .header("User-Agent", "1panel-client");
    if let Some(key) = key {
        request = request.bearer_auth(key.expose_secret());
    }
    let mut response = if let Some(guard) = task_guard.as_ref() {
        if guard.is_cancelled() {
            return Err(AppError::new("CANCELLED", "task", "AI 流式请求已取消"));
        }
        tokio::select! {
            result = request.send() => result.map_err(|error| AppError::new("AI_NETWORK_FAILED", "ai", "AI 请求失败").details(error))?,
            _ = guard.wait_cancelled() => return Err(AppError::new("CANCELLED", "task", "AI 流式请求已取消")),
        }
    } else {
        request.send().await.map_err(|error| {
            AppError::new("AI_NETWORK_FAILED", "ai", "AI 请求失败").details(error)
        })?
    };
    let status = response.status();
    if !status.is_success() {
        let payload = response.text().await.unwrap_or_default();
        return Err(AppError::new(
            "AI_REQUEST_FAILED",
            "ai",
            format!("AI 服务返回 HTTP {}", status.as_u16()),
        )
        .details(crate::security::redact(&payload)));
    }
    let mut buffer = String::new();
    let mut content = String::new();
    let mut model = provider.model.clone();
    let mut usage = None;
    loop {
        let chunk = if let Some(guard) = task_guard.as_ref() {
            if guard.is_cancelled() {
                return Err(AppError::new("CANCELLED", "task", "AI 流式请求已取消"));
            }
            tokio::select! {
                result = response.chunk() => result.map_err(|error| AppError::new("AI_NETWORK_FAILED", "ai", "AI 流式响应读取失败").details(error))?,
                _ = guard.wait_cancelled() => return Err(AppError::new("CANCELLED", "task", "AI 流式请求已取消")),
            }
        } else {
            response.chunk().await.map_err(|error| {
                AppError::new("AI_NETWORK_FAILED", "ai", "AI 流式响应读取失败").details(error)
            })?
        };
        let Some(chunk) = chunk else {
            break;
        };
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(position) = buffer.find('\n') {
            let line = buffer[..position].trim_end_matches('\r').to_string();
            buffer.drain(..=position);
            if parse_stream_line(&line, &mut content, &mut model, &mut usage, events)? {
                break;
            }
        }
    }
    if !buffer.trim().is_empty() {
        let _ = parse_stream_line(
            buffer.trim_end_matches('\r'),
            &mut content,
            &mut model,
            &mut usage,
            events,
        )?;
    }
    let prompt_tokens = usage.as_ref().and_then(|value| value.prompt_tokens);
    let completion_tokens = usage.as_ref().and_then(|value| value.completion_tokens);
    events
        .send(AiStreamEvent::Completed {
            model: model.clone(),
            prompt_tokens,
            completion_tokens,
        })
        .map_err(|error| {
            AppError::new("AI_STREAM_CHANNEL_CLOSED", "ai", "AI 流式通道已关闭").details(error)
        })?;
    if content.trim().is_empty() {
        return Err(AppError::new("AI_RESPONSE_EMPTY", "ai", "AI 没有返回文本"));
    }
    Ok(AiChatResult {
        provider_id: provider.id,
        model,
        content,
        prompt_tokens,
        completion_tokens,
    })
}

/// 解析单行 SSE data，并把可见 delta 立即发送到前端；返回 true 表示流已结束。
fn parse_stream_line(
    line: &str,
    content: &mut String,
    model: &mut String,
    usage: &mut Option<ChatUsage>,
    events: &Channel<AiStreamEvent>,
) -> AppResult<bool> {
    let Some(payload) = line.strip_prefix("data:").map(str::trim) else {
        return Ok(false);
    };
    if payload.is_empty() {
        return Ok(false);
    }
    if payload == "[DONE]" {
        return Ok(true);
    }
    let parsed = serde_json::from_str::<StreamResponse>(payload).map_err(|error| {
        AppError::new("AI_RESPONSE_INVALID", "ai", "AI 流式响应格式无效").details(error)
    })?;
    if let Some(value) = parsed.model.filter(|value| !value.trim().is_empty()) {
        *model = value;
    }
    if parsed.usage.is_some() {
        *usage = parsed.usage;
    }
    for choice in parsed.choices {
        if let Some(delta) = choice.delta.and_then(|value| value.content) {
            if delta.is_empty() {
                continue;
            }
            content.push_str(&delta);
            events
                .send(AiStreamEvent::Delta { content: delta })
                .map_err(|error| {
                    AppError::new("AI_STREAM_CHANNEL_CLOSED", "ai", "AI 流式通道已关闭")
                        .details(error)
                })?;
        }
    }
    Ok(false)
}

/// 读取并校验本地供应商 JSON；损坏配置不会静默降级为远端 Mock。
async fn load_stored(local: &LocalRepository) -> AppResult<Vec<StoredProvider>> {
    let value = local.get_setting(SETTINGS_KEY).await?;
    value
        .map(|value| {
            serde_json::from_str(&value).map_err(|error| {
                AppError::new("AI_CONFIG_INVALID", "ai", "AI 配置无法解析").details(error)
            })
        })
        .transpose()
        .map(|values| values.unwrap_or_default())
}

/// 读取并校验本地对话 JSON；损坏历史会显式报错，避免静默丢失用户消息。
async fn load_conversations(local: &LocalRepository) -> AppResult<Vec<AiConversation>> {
    let value = local.get_setting(CONVERSATIONS_KEY).await?;
    value
        .map(|value| {
            serde_json::from_str::<Vec<AiConversation>>(&value).map_err(|error| {
                AppError::new("AI_HISTORY_INVALID", "ai", "AI 对话历史无法解析").details(error)
            })
        })
        .transpose()
        .map(|values| values.unwrap_or_default())
}

/// 序列化并保存有界对话历史；超过本地上限时拒绝写入而不截断用户消息。
async fn persist_conversations(
    local: &LocalRepository,
    conversations: &[AiConversation],
) -> AppResult<()> {
    if conversations.is_empty() {
        return local.delete_setting(CONVERSATIONS_KEY).await;
    }
    let json = serde_json::to_string(conversations).map_err(AppError::database)?;
    if json.len() > MAX_HISTORY_BYTES {
        return Err(AppError::new(
            "AI_HISTORY_TOO_LARGE",
            "ai",
            "AI 对话历史超过本地存储上限，请先清理旧会话",
        ));
    }
    local.set_setting(CONVERSATIONS_KEY, &json).await
}

/// 校验本地会话输入，复用聊天消息的角色、数量和内容长度边界。
fn validate_conversation(input: &SaveAiConversationInput) -> AppResult<()> {
    if input.id.as_deref().is_some_and(|value| !valid_id(value)) || !valid_id(&input.provider_id) {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "AI 对话标识无效"));
    }
    if input.title.as_deref().is_some_and(|value| {
        value.trim().is_empty()
            || value.chars().count() > 120
            || value.chars().any(char::is_control)
    }) {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "AI 对话标题无效"));
    }
    validate_chat(&AiChatInput {
        provider_id: input.provider_id.clone(),
        messages: input.messages.clone(),
        temperature: None,
        task_id: None,
    })
}

/// 从首条用户消息生成稳定标题，避免把整段提示词重复写入标题字段。
fn conversation_title(title: Option<&str>, messages: &[AiChatMessage]) -> String {
    if let Some(title) = title.map(str::trim).filter(|value| !value.is_empty()) {
        return title.chars().take(120).collect();
    }
    messages
        .iter()
        .find(|message| message.role == "user")
        .map(|message| {
            message
                .content
                .lines()
                .next()
                .unwrap_or("新对话")
                .chars()
                .take(80)
                .collect()
        })
        .filter(|value: &String| !value.trim().is_empty())
        .unwrap_or_else(|| "新对话".into())
}

/// 生成密钥链引用名，不把 URL、模型或用户输入放进引用中。
fn key_ref(id: &str) -> String {
    format!("{KEY_PREFIX}{id}")
}

/// 校验供应商字段和协议，允许本地 Ollama 的 HTTP 地址。
fn validate_provider(input: &SaveAiProviderInput) -> AppResult<()> {
    if input.name.trim().is_empty()
        || input.name.len() > 80
        || input.model.trim().is_empty()
        || input.model.len() > 160
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI 供应商名称和模型不能为空",
        ));
    }
    let value = input.base_url.trim();
    if !(value.starts_with("https://") || value.starts_with("http://"))
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI Base URL 必须是 HTTP(S) 地址",
        ));
    }
    Ok(())
}

/// 校验聊天消息的角色、内容长度和请求数量。
fn validate_chat(input: &AiChatInput) -> AppResult<()> {
    if !valid_id(&input.provider_id) || input.messages.is_empty() || input.messages.len() > 100 {
        return Err(AppError::new("VALIDATION_FAILED", "ai", "AI 聊天请求无效"));
    }
    if input.task_id.as_deref().is_some_and(|value| {
        value.is_empty()
            || value.len() > 128
            || value
                .chars()
                .any(|character| character == '\0' || character == '\r' || character == '\n')
    }) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI 流式任务标识无效",
        ));
    }
    if input.messages.iter().any(|message| {
        !matches!(message.role.as_str(), "system" | "user" | "assistant")
            || message.content.trim().is_empty()
            || message.content.len() > 128 * 1024
    }) {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "AI 消息角色或长度无效",
        ));
    }
    Ok(())
}

/// Validates the agent request and keeps tool execution bound to one non-empty server id.
fn validate_agent(input: &AiAgentInput) -> AppResult<()> {
    if input.server_id.trim().is_empty() || input.server_id.len() > 128 {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "智能体服务器标识无效",
        ));
    }
    validate_chat(&AiChatInput {
        provider_id: input.provider_id.clone(),
        messages: input.messages.clone(),
        temperature: Some(0.2),
        task_id: None,
    })?;
    if input
        .max_steps
        .is_some_and(|steps| !(1..=6).contains(&steps))
    {
        return Err(AppError::new(
            "VALIDATION_FAILED",
            "ai",
            "智能体步数必须在 1-6 范围内",
        ));
    }
    Ok(())
}

/// 规范化 Base URL，保留用户输入的 /v1 兼容路径。
fn normalize_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

/// 将 OpenAI Base URL 补齐为 chat/completions endpoint。
fn chat_endpoint(base_url: &str) -> String {
    if base_url.ends_with("/chat/completions") {
        base_url.to_string()
    } else if base_url.ends_with("/v1") {
        format!("{base_url}/chat/completions")
    } else {
        format!("{base_url}/v1/chat/completions")
    }
}

/// 将 OpenAI-compatible Base URL 补齐为 models endpoint。
fn models_endpoint(base_url: &str) -> String {
    if base_url.ends_with("/v1") {
        format!("{base_url}/models")
    } else if base_url.ends_with("/v1/chat/completions") || base_url.ends_with("/chat/completions")
    {
        format!("{}/models", base_url.trim_end_matches("/chat/completions"))
    } else if base_url.ends_with("/models") {
        base_url.to_string()
    } else {
        format!("{base_url}/v1/models")
    }
}

/// 校验供应商 ID 只包含安全目录字符。
fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

/// 为缺少 enabled 字段的旧配置提供默认启用状态。
fn default_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{
        agent_tools, chat_endpoint, conversation_title, models_endpoint, normalize_base_url,
        parse_stream_line, valid_id, validate_agent, validate_conversation, AiAgentInput,
        AiChatMessage, Channel, SaveAiConversationInput,
    };

    #[test]
    fn builds_compatible_chat_endpoint() {
        assert_eq!(
            chat_endpoint("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_endpoint("http://localhost:11434/v1"),
            "http://localhost:11434/v1/chat/completions"
        );
        assert_eq!(
            models_endpoint("https://api.openai.com/v1"),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn validates_local_ids() {
        assert!(valid_id("provider-openai"));
        assert!(!valid_id("../provider"));
        assert_eq!(
            normalize_base_url("https://example.com/"),
            "https://example.com"
        );
    }

    #[test]
    fn parses_sse_delta_and_completion_marker() {
        let events = Channel::new(|_| Ok(()));
        let mut content = String::new();
        let mut model = "fallback".to_string();
        let mut usage = None;
        assert!(!parse_stream_line(
            r#"data: {"model":"demo","choices":[{"delta":{"content":"你好"}}]}"#,
            &mut content,
            &mut model,
            &mut usage,
            &events,
        )
        .unwrap());
        assert_eq!(content, "你好");
        assert_eq!(model, "demo");
        assert!(parse_stream_line(
            "data: [DONE]",
            &mut content,
            &mut model,
            &mut usage,
            &events,
        )
        .unwrap());
    }

    #[test]
    fn validates_agent_step_bounds_and_server_scope() {
        let input = AiAgentInput {
            provider_id: "provider".into(),
            server_id: "server".into(),
            messages: vec![AiChatMessage {
                role: "user".into(),
                content: "读取状态".into(),
            }],
            max_steps: Some(4),
            mcp_enabled: false,
        };
        assert!(validate_agent(&input).is_ok());
        let mut invalid = input;
        invalid.max_steps = Some(7);
        assert!(validate_agent(&invalid).is_err());
    }

    #[test]
    fn exposes_only_read_only_builtin_agent_tools() {
        let tools = agent_tools(None);
        let names = tools
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool["function"]["name"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "server_overview",
                "server_websites",
                "server_docker",
                "server_security",
                "server_processes"
            ]
        );
        assert!(names.iter().all(|name| !name.contains("delete")));
    }

    /// 验证本地对话复用聊天消息边界，并从首条用户消息生成有限标题。
    #[test]
    fn validates_conversation_history_without_secrets() {
        let input = SaveAiConversationInput {
            id: Some("conversation-1".into()),
            provider_id: "provider".into(),
            title: None,
            messages: vec![AiChatMessage {
                role: "user".into(),
                content: "请检查服务器状态\n并返回摘要".into(),
            }],
        };
        assert!(validate_conversation(&input).is_ok());
        assert_eq!(
            conversation_title(None, &input.messages),
            "请检查服务器状态"
        );
        let mut invalid = input;
        invalid.messages[0].role = "tool".into();
        assert!(validate_conversation(&invalid).is_err());
    }
}
