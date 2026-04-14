// 工作流数据模型定义

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const WORKFLOW_CONTRACT_VERSION: u32 = 2;

/// 工作流节点定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub skip_when: Vec<String>,
    #[serde(default)]
    pub action: String,
    /// 循环回退目标节点 ID（仅对触发循环的节点有效，如 gate）
    #[serde(default)]
    pub loop_back_to: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Complexity {
    Simple,
    Medium,
    Complex,
}

impl std::fmt::Display for Complexity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Complexity::Simple => write!(f, "simple"),
            Complexity::Medium => write!(f, "medium"),
            Complexity::Complex => write!(f, "complex"),
        }
    }
}

impl Complexity {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "simple" => Some(Complexity::Simple),
            "medium" => Some(Complexity::Medium),
            "complex" => Some(Complexity::Complex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityRule {
    pub max_files: Option<u32>,
    #[serde(default)]
    pub nature: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub complexity_rules: HashMap<String, ComplexityRule>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowValidationSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowValidationIssue {
    pub code: String,
    pub severity: WorkflowValidationSeverity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowConfigMetadata {
    pub workflow_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_path: Option<String>,
    pub has_errors: bool,
    pub issues: Vec<WorkflowValidationIssue>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowHintResult {
    pub contract_version: u32,
    pub complexity: String,
    pub workflow_fingerprint: String,
    pub hint_fingerprint: String,
    pub expected_step_ids: Vec<String>,
    pub suggested_steps: Vec<SuggestedStep>,
    pub skipped_steps: Vec<SkippedStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_info: Option<LoopInfo>,
    pub reminder: String,
    pub progress_display: String,
}

/// 循环回退信息
#[derive(Debug, Clone, Serialize)]
pub struct LoopInfo {
    /// 触发循环的节点 ID（如 gate）
    pub loop_node_id: String,
    /// 循环回退的目标节点 ID
    pub loop_back_to: String,
    /// 循环时需要重新执行的节点 ID 列表（从 loop_back_to 到 loop_node 之间的建议步骤）
    pub re_execute_nodes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedStep {
    pub id: String,
    pub name: String,
    pub action: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip_conditions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkippedStep {
    pub id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Ok,
    MissingSteps,
    StaleConfig,
    InvalidHintSnapshot,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowCheckResult {
    pub contract_version: u32,
    pub status: CheckStatus,
    pub passed: bool,
    pub should_rehint: bool,
    pub missing_steps: Vec<MissingStep>,
    pub missing_required_steps: Vec<MissingStep>,
    pub completed_steps: Vec<String>,
    pub normalized_completed_steps: Vec<String>,
    pub unknown_completed_steps: Vec<String>,
    pub duplicate_completed_steps: Vec<String>,
    pub expected_step_ids: Vec<String>,
    pub current_workflow_fingerprint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provided_workflow_fingerprint: Option<String>,
    pub hint_fingerprint: String,
    pub completion_rate: f32,
    pub diagnostics: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loop_info: Option<LoopInfo>,
    pub message: String,
    pub progress_display: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MissingStep {
    pub id: String,
    pub name: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowPreset {
    pub name: String,
    pub description: String,
    pub workflow: WorkflowDefinition,
}
