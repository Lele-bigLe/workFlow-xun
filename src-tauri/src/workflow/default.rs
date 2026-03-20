// 内置默认工作流定义
use std::collections::HashMap;
use super::definition::{WorkflowDefinition, WorkflowNode, ComplexityRule, WorkflowPreset};

pub fn default_workflow() -> WorkflowDefinition {
    get_presets().into_iter().next().unwrap().workflow
}

pub fn get_presets() -> Vec<WorkflowPreset> {
    let mut rules = HashMap::new();
    rules.insert(
        "simple".to_string(),
        ComplexityRule {
            max_files: Some(2),
            nature: vec![
                "register".to_string(),
                "config".to_string(),
                "single_edit".to_string(),
                "fix_typo".to_string(),
                "add_field".to_string(),
            ],
        },
    );
    rules.insert(
        "medium".to_string(),
        ComplexityRule {
            max_files: Some(5),
            nature: vec![
                "new_feature".to_string(),
                "multi_file".to_string(),
                "refactor".to_string(),
                "bug_fix".to_string(),
            ],
        },
    );
    rules.insert(
        "complex".to_string(),
        ComplexityRule {
            max_files: None,
            nature: vec![
                "architecture".to_string(),
                "breaking_change".to_string(),
                "migration".to_string(),
                "redesign".to_string(),
            ],
        },
    );

    vec![
        WorkflowPreset {
            name: "均衡编码流 (标准推荐)".to_string(),
            description: "默认推荐套件。兼顾背景搜索、方案设计、编码与成果验证。".to_string(),
            workflow: WorkflowDefinition {
                nodes: vec![
                    WorkflowNode {
                        id: "memory_gate".to_string(),
                        name: "记忆搜索".to_string(),
                        required: false,
                        skip_when: vec!["用户没有明确的任务意图时跳过".to_string(), "用户只是打招呼问候时跳过".to_string()],
                        action: "调用 smart_search 搜索相关记忆，命中则 memory_read".to_string(),
                        loop_back_to: None,
                    },
                    WorkflowNode {
                        id: "read_context".to_string(),
                        name: "上下文读取".to_string(),
                        required: true,
                        skip_when: vec![],
                        action: "读取相关代码文件，理解现有实现".to_string(),
                        loop_back_to: None,
                    },
                    WorkflowNode {
                        id: "plan_design".to_string(),
                        name: "方案设计".to_string(),
                        required: false,
                        skip_when: vec!["complexity:simple".to_string(), "complexity:medium".to_string()],
                        action: "使用 sequential-thinking 分析方案（≤3步）".to_string(),
                        loop_back_to: None,
                    },
                    WorkflowNode {
                        id: "execute".to_string(),
                        name: "代码实现".to_string(),
                        required: true,
                        skip_when: vec![],
                        action: "执行代码修改，遵循项目代码规范".to_string(),
                        loop_back_to: None,
                    },
                    WorkflowNode {
                        id: "verify".to_string(),
                        name: "自检验证".to_string(),
                        required: false,
                        skip_when: vec!["complexity:simple".to_string(), "仅做配置类修改时跳过".to_string()],
                        action: "运行 lint/build/typecheck 验证变更".to_string(),
                        loop_back_to: None,
                    },
                    WorkflowNode {
                        id: "gate".to_string(),
                        name: "交付确认".to_string(),
                        required: true,
                        skip_when: vec![],
                        action: "调用寸止(zhi)工具获取用户反馈，循环直到用户说'结束'".to_string(),
                        loop_back_to: Some("execute".to_string()),
                    },
                ],
                complexity_rules: rules.clone(),
            }
        },
        WorkflowPreset {
            name: "轻量查阅流 (仅阅读)".to_string(),
            description: "不附带任何修改动作，专用于代码架构梳理、Bug 定位等纯查阅需求。".to_string(),
            workflow: WorkflowDefinition {
                nodes: vec![
                    WorkflowNode {
                        id: "info_gather".to_string(),
                        name: "信息收集".to_string(),
                        required: true,
                        skip_when: vec![],
                        action: "调用 file_search 和 grep_search 海量收集相关逻辑".to_string(),
                        loop_back_to: None,
                    },
                    WorkflowNode {
                        id: "deep_read".to_string(),
                        name: "深度阅读".to_string(),
                        required: true,
                        skip_when: vec![],
                        action: "使用 view_file 精读可疑或相关的源文件".to_string(),
                        loop_back_to: None,
                    },
                    WorkflowNode {
                        id: "analysis_report".to_string(),
                        name: "分析结论输出".to_string(),
                        required: true,
                        skip_when: vec![],
                        action: "输出总结报告，并调用交互工具报告结果".to_string(),
                        loop_back_to: Some("deep_read".to_string()),
                    },
                ],
                complexity_rules: rules.clone(),
            }
        }
    ]
}
