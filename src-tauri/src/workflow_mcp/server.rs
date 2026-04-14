// 循（Xun）MCP Server 实现

use anyhow::Result;
use rmcp::{
    Error as McpError, ServerHandler, ServiceExt, RoleServer,
    model::*,
    transport::stdio,
    service::RequestContext,
};
use std::sync::Arc;
use std::borrow::Cow;

use crate::workflow::{
    WorkflowHintResult,
    SuggestedStep,
    evaluate_workflow, check_workflow,
};

#[derive(Debug, serde::Deserialize)]
struct HintRequest {
    task_description: String,
    #[serde(default)]
    complexity: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct CheckRequest {
    task_description: String,
    #[serde(default)]
    complexity: Option<String>,
    completed_steps: Vec<String>,
    #[serde(default)]
    expected_steps: Vec<SuggestedStep>,
    #[serde(default)]
    workflow_fingerprint: Option<String>,
    #[serde(default)]
    hint_fingerprint: Option<String>,
}

#[derive(Clone)]
pub struct XunServer;

impl XunServer {
    pub fn new() -> Self {
        Self
    }
}

impl Default for XunServer {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerHandler for XunServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "workFlow".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
            instructions: Some(
                "工作流协议：\n\
                1. 首次会话或刚重启时，先确认工具列表里已出现 `hint`/`check`，未出现时先等待 MCP 注册完成。\n\
                2. 任务开始先调 `hint`，按 `suggested_steps` 执行并更新进度。\n\
                3. 任务完成前调 `check`。若返回 `should_rehint=true`、`stale_config` 或 `invalid_hint_snapshot`，必须重跑 `hint`。\n\
                4. 结束前调用寸止(zhi)收集反馈，直到用户说结束。\n"
                    .to_string(),
            ),
        }
    }

    async fn initialize(
        &self,
        _request: InitializeRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ServerInfo, McpError> {
        Ok(self.get_info())
    }

    async fn list_tools(
        &self,
        _request: PaginatedRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let mut tools = Vec::new();

        let hint_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "task_description": {
                    "type": "string",
                    "description": "一句话任务描述"
                },
                "complexity": {
                    "type": "string",
                    "enum": ["simple", "medium", "complex"],
                    "description": "可选复杂度"
                }
            },
            "required": ["task_description"]
        });

        if let serde_json::Value::Object(schema_map) = hint_schema {
            tools.push(Tool {
                name: Cow::Borrowed("hint"),
                description: Cow::Borrowed(
                    "任务开始前调用。返回 complexity、suggested_steps、skipped_steps、loop_info、workflow_fingerprint、hint_fingerprint。"
                ),
                input_schema: Arc::new(schema_map),
            });
        }

        let check_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "task_description": {
                    "type": "string",
                    "description": "与 hint 一致的任务描述"
                },
                "complexity": {
                    "type": "string",
                    "enum": ["simple", "medium", "complex"],
                    "description": "可选复杂度"
                },
                "expected_steps": {
                    "type": "array",
                    "description": "可选，回传 hint 的 suggested_steps",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": { "type": "string" },
                            "name": { "type": "string" },
                            "action": { "type": "string" },
                            "skip_conditions": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        },
                        "required": ["id", "name", "action"]
                    }
                },
                "workflow_fingerprint": {
                    "type": "string",
                    "description": "可选，回传 hint 的 workflow_fingerprint"
                },
                "hint_fingerprint": {
                    "type": "string",
                    "description": "可选，回传 hint 的 hint_fingerprint"
                },
                "completed_steps": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "已完成步骤 ID 列表"
                }
            },
            "required": ["task_description", "completed_steps"]
        });

        if let serde_json::Value::Object(schema_map) = check_schema {
            tools.push(Tool {
                name: Cow::Borrowed("check"),
                description: Cow::Borrowed(
                    "任务完成前调用。尽量回传 hint 的 suggested_steps、workflow_fingerprint、hint_fingerprint。返回 status、passed、should_rehint、missing_steps、diagnostics。"
                ),
                input_schema: Arc::new(schema_map),
            });
        }

        Ok(ListToolsResult {
            tools,
            next_cursor: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let arguments_value = request
            .arguments
            .map(serde_json::Value::Object)
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        match request.name.as_ref() {
            "hint" => {
                let hint_request: HintRequest = match serde_json::from_value(arguments_value) {
                    Ok(req) => req,
                    Err(e) => {
                        let msg = format!("hint 参数解析失败: {}。请确保传入 task_description(string) 字段。", e);
                        log::warn!("{}", msg);
                        return Ok(CallToolResult::error(vec![Content::text(msg)]));
                    }
                };

                let workflow_def = crate::workflow::loader::load_workflow_definition();

                let result: WorkflowHintResult = evaluate_workflow(
                    &workflow_def,
                    &hint_request.task_description,
                    hint_request.complexity.as_deref(),
                );

                let result_json = match serde_json::to_string(&result) {
                    Ok(json) => json,
                    Err(e) => {
                        let msg = format!("hint 结果序列化失败: {}", e);
                        log::error!("{}", msg);
                        return Ok(CallToolResult::error(vec![Content::text(msg)]));
                    }
                };

                log::debug!(
                    "hint: task=\"{}\" → complexity={}",
                    hint_request.task_description,
                    result.complexity
                );

                Ok(CallToolResult::success(vec![Content::text(result_json)]))
            }
            "check" => {
                let check_request: CheckRequest = match serde_json::from_value(arguments_value) {
                    Ok(req) => req,
                    Err(e) => {
                        let msg = format!("check 参数解析失败: {}。请确保传入 task_description(string) 和 completed_steps(array) 字段。", e);
                        log::warn!("{}", msg);
                        return Ok(CallToolResult::error(vec![Content::text(msg)]));
                    }
                };

                let workflow_def = crate::workflow::loader::load_workflow_definition();

                let result = check_workflow(
                    &workflow_def,
                    &check_request.task_description,
                    check_request.complexity.as_deref(),
                    &check_request.completed_steps,
                    Some(&check_request.expected_steps),
                    check_request.workflow_fingerprint.as_deref(),
                    check_request.hint_fingerprint.as_deref(),
                );

                let result_json = match serde_json::to_string(&result) {
                    Ok(json) => json,
                    Err(e) => {
                        let msg = format!("check 结果序列化失败: {}", e);
                        log::error!("{}", msg);
                        return Ok(CallToolResult::error(vec![Content::text(msg)]));
                    }
                };

                log::debug!(
                    "check: status={:?}, passed={}, missing={}",
                    result.status,
                    result.passed,
                    result.missing_steps.len()
                );

                Ok(CallToolResult::success(vec![Content::text(result_json)]))
            }
            _ => {
                let msg = format!("未知的工具: {}。可用工具: hint, check", request.name);
                log::warn!("{}", msg);
                Ok(CallToolResult::error(vec![Content::text(msg)]))
            }
        }
    }
}

pub async fn run_workflow_server() -> Result<(), Box<dyn std::error::Error>> {
    let server = XunServer::new();
    log::debug!("XunServer 初始化完成，准备启动 stdio 传输");

    let service = server
        .serve(stdio())
        .await
        .inspect_err(|e| {
            log::error!("启动循(Xun) MCP 服务器失败: {}", e);
        })?;

    log::debug!("MCP stdio 传输已建立，等待请求...");
    service.waiting().await?;
    log::debug!("MCP 服务器正常退出");
    Ok(())
}
