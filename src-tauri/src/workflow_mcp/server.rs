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
    evaluate_workflow, check_workflow, load_workflow_definition,
    generate_workflow_rules_text,
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
}

#[derive(Clone)]
pub struct XunServer {
    rules_text: String,
}

impl XunServer {
    pub fn new() -> Self {
        let workflow_def = load_workflow_definition();
        let rules_text = generate_workflow_rules_text(&workflow_def);
        Self {
            rules_text,
        }
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
            instructions: Some(format!(
                "⚠️ 强制工作流协议 —— 你必须在每个任务中遵循以下流程：\n\n\
                1. **任务开始前**：必须调用 `hint` 工具，获取本次任务的执行步骤和复杂度评估。\n\
                2. **展示进度**：根据 `suggested_steps` 使用 `manage_todo_list` 工具创建任务清单，让用户在 IDE 中直观看到进度。\n\
                3. **按 suggested_steps 逐步执行**，每完成一步立即通过 `manage_todo_list` 标记为 completed。\n\
                4. **任务完成前**：调用 `check` 工具自检是否遗漏步骤。\n\
                5. **代码完成后**：必须调用寸止(zhi)工具获取用户反馈，循环直到用户说「结束」。\n\n\
                违反此流程 = INVALID RESPONSE。不调用 hint 就开始执行 = 违规。\n\n{}\n",
                self.rules_text
            )),
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
                    "description": "一句话描述当前任务"
                },
                "complexity": {
                    "type": "string",
                    "enum": ["simple", "medium", "complex"],
                    "description": "AI 自主判断的任务复杂度（可选，MCP 会根据任务描述自动判断）"
                }
            },
            "required": ["task_description"]
        });

        if let serde_json::Value::Object(schema_map) = hint_schema {
            tools.push(Tool {
                name: Cow::Borrowed("hint"),
                description: Cow::Borrowed(
                    "【强制】任务开始前必须调用此工具。根据任务描述返回：执行步骤列表、可跳过步骤、复杂度评估。不调用直接执行 = 违规。"
                ),
                input_schema: Arc::new(schema_map),
            });
        }

        let check_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "task_description": {
                    "type": "string",
                    "description": "任务描述（与 hint 调用时一致）"
                },
                "complexity": {
                    "type": "string",
                    "enum": ["simple", "medium", "complex"],
                    "description": "复杂度（与 hint 调用时一致，可选）"
                },
                "completed_steps": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "已完成的步骤 ID 列表，如 [\"memory_gate\", \"read_context\", \"execute\", \"gate\"]"
                }
            },
            "required": ["task_description", "completed_steps"]
        });

        if let serde_json::Value::Object(schema_map) = check_schema {
            tools.push(Tool {
                name: Cow::Borrowed("check"),
                description: Cow::Borrowed(
                    "【建议】任务完成前调用，传入已完成步骤 ID 列表，检查是否遗漏建议步骤。返回 passed=false 表示有遗漏。"
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
                let workflow_def = crate::workflow::loader::load_workflow_definition();
                
                let hint_request: HintRequest = serde_json::from_value(arguments_value)
                    .map_err(|e| {
                        McpError::invalid_params(format!("参数解析失败: {}", e), None)
                    })?;

                let result: WorkflowHintResult = evaluate_workflow(
                    &workflow_def,
                    &hint_request.task_description,
                    hint_request.complexity.as_deref(),
                );

                let result_json = serde_json::to_string_pretty(&result)
                    .map_err(|e| {
                        McpError::internal_error(format!("结果序列化失败: {}", e), None)
                    })?;

                log::info!(
                    "hint: task=\"{}\" → complexity={}",
                    hint_request.task_description,
                    result.complexity
                );

                Ok(CallToolResult::success(vec![Content::text(result_json)]))
            }
            "check" => {
                let workflow_def = crate::workflow::loader::load_workflow_definition();
                
                let check_request: CheckRequest = serde_json::from_value(arguments_value)
                    .map_err(|e| {
                        McpError::invalid_params(format!("参数解析失败: {}", e), None)
                    })?;

                let result = check_workflow(
                    &workflow_def,
                    &check_request.task_description,
                    check_request.complexity.as_deref(),
                    &check_request.completed_steps,
                );

                let result_json = serde_json::to_string_pretty(&result)
                    .map_err(|e| {
                        McpError::internal_error(format!("结果序列化失败: {}", e), None)
                    })?;

                log::info!(
                    "check: passed={}, missing={}",
                    result.passed,
                    result.missing_steps.len()
                );

                Ok(CallToolResult::success(vec![Content::text(result_json)]))
            }
            _ => Err(McpError::invalid_request(
                format!("未知的工具: {}", request.name),
                None,
            )),
        }
    }
}

pub async fn run_workflow_server() -> Result<(), Box<dyn std::error::Error>> {
    let service = XunServer::new()
        .serve(stdio())
        .await
        .inspect_err(|e| {
            log::error!("启动循(Xun) MCP 服务器失败: {}", e);
        })?;

    service.waiting().await?;
    Ok(())
}
