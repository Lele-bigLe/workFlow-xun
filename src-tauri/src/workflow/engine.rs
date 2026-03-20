// 工作流引擎
// 根据任务描述评估复杂度，生成工作流建议路径

use super::definition::*;

pub fn evaluate_workflow(
    definition: &WorkflowDefinition,
    task_description: &str,
    hint_complexity: Option<&str>,
) -> WorkflowHintResult {
    let complexity = determine_complexity(definition, task_description, hint_complexity);
    let complexity_tag = format!("complexity:{}", complexity);

    // 内置条件代号集合，用于区分哪些是引擎可处理的内置条件
    let builtin_conditions: &[&str] = &[
        "no_task_intent", "simple_greeting", "config_only",
    ];

    let mut suggested_steps = Vec::new();
    let mut skipped_steps = Vec::new();

    for node in &definition.nodes {
        let skip_reason = should_skip_node(node, &complexity_tag, task_description);

        if let Some(reason) = skip_reason {
            skipped_steps.push(SkippedStep {
                id: node.id.clone(),
                reason,
            });
        } else {
            // 收集非内置的自定义白话条件，透传给 AI 做自主判断
            let custom_conditions: Vec<String> = node.skip_when.iter()
                .filter(|c| !c.starts_with("complexity:") && !builtin_conditions.contains(&c.as_str()))
                .cloned()
                .collect();

            suggested_steps.push(SuggestedStep {
                id: node.id.clone(),
                name: node.name.clone(),
                action: node.action.clone(),
                skip_conditions: custom_conditions,
            });
        }
    }

    let reminder = build_reminder(&complexity);

    let progress_display = build_progress_display(&suggested_steps, &[]);

    // 构建循环回退信息
    let loop_info = build_loop_info(&definition.nodes, &suggested_steps);

    WorkflowHintResult {
        complexity: complexity.to_string(),
        suggested_steps,
        skipped_steps,
        loop_info,
        reminder,
        progress_display,
    }
}

/// 从节点定义中提取循环回退信息
fn build_loop_info(nodes: &[WorkflowNode], suggested_steps: &[SuggestedStep]) -> Option<LoopInfo> {
    // 找到有 loop_back_to 的节点
    let loop_node = nodes.iter().find(|n| n.loop_back_to.is_some())?;
    let target_id = loop_node.loop_back_to.as_ref()?;

    // 收集 suggested_steps 中从 target 到 loop_node 之间的节点 ID
    let suggested_ids: Vec<&str> = suggested_steps.iter().map(|s| s.id.as_str()).collect();
    let target_pos = suggested_ids.iter().position(|id| *id == target_id.as_str());
    let loop_pos = suggested_ids.iter().position(|id| *id == loop_node.id.as_str());

    let re_execute_nodes = match (target_pos, loop_pos) {
        (Some(start), Some(end)) if start <= end => {
            suggested_ids[start..=end].iter().map(|s| s.to_string()).collect()
        }
        _ => {
            // target 被跳过了或顺序异常，回退到全部 suggested_steps
            suggested_ids.iter().map(|s| s.to_string()).collect()
        }
    };

    Some(LoopInfo {
        loop_node_id: loop_node.id.clone(),
        loop_back_to: target_id.clone(),
        re_execute_nodes,
    })
}

fn determine_complexity(
    definition: &WorkflowDefinition,
    task_description: &str,
    hint_complexity: Option<&str>,
) -> Complexity {
    if let Some(hint) = hint_complexity {
        if let Some(c) = Complexity::from_str_opt(hint) {
            return c;
        }
    }

    let desc_lower = task_description.to_lowercase();

    if matches_complexity_rule(definition, "complex", &desc_lower) {
        return Complexity::Complex;
    }
    if matches_complexity_rule(definition, "medium", &desc_lower) {
        return Complexity::Medium;
    }

    Complexity::Simple
}

fn matches_complexity_rule(
    definition: &WorkflowDefinition,
    level: &str,
    desc_lower: &str,
) -> bool {
    if let Some(rule) = definition.complexity_rules.get(level) {
        for keyword in &rule.nature {
            if desc_lower.contains(&keyword.to_lowercase()) {
                return true;
            }
        }
    }
    false
}

fn should_skip_node(
    node: &WorkflowNode,
    complexity_tag: &str,
    task_description: &str,
) -> Option<String> {
    if node.required {
        return None;
    }

    let desc_lower = task_description.to_lowercase();

    for condition in &node.skip_when {
        if condition.starts_with("complexity:") && condition == complexity_tag {
            let level = condition.strip_prefix("complexity:").unwrap_or("");
            return Some(format!("{}任务可跳过「{}」", level, node.name));
        }

        let cond_lower = condition.to_lowercase();

        // 无任务意图判断（兼容旧代号 + 新白话）
        if condition == "no_task_intent" || cond_lower.contains("任务意图") {
            let task_keywords = ["修改", "添加", "删除", "修复", "实现", "重构", "创建",
                                 "fix", "add", "delete", "implement", "refactor", "create",
                                 "update", "change", "move", "remove", "build"];
            if !task_keywords.iter().any(|k| desc_lower.contains(k)) {
                return Some(format!("无明确任务意图，跳过「{}」", node.name));
            }
            continue;
        }

        // 简单问候判断（兼容旧代号 + 新白话）
        if condition == "simple_greeting" || cond_lower.contains("打招呼") || cond_lower.contains("问候") {
            let greetings = ["你好", "hello", "hi", "hey", "嗨"];
            if greetings.iter().any(|g| desc_lower.starts_with(g)) && desc_lower.len() < 20 {
                return Some(format!("简单问候，跳过「{}」", node.name));
            }
            continue;
        }

        // 仅配置变更判断（兼容旧代号 + 新白话）
        if condition == "config_only" || cond_lower.contains("配置") && cond_lower.contains("跳过") {
            let config_keywords = ["配置", "config", "设置", "env", ".env", "yaml", "json配置"];
            if config_keywords.iter().any(|k| desc_lower.contains(k))
                && !desc_lower.contains("重构")
                && !desc_lower.contains("架构")
            {
                return Some(format!("仅配置变更，跳过「{}」", node.name));
            }
            continue;
        }

        // 其他自定义条件：透传给 AI 判研（不在引擎层判断）
    }

    None
}

fn build_reminder(complexity: &Complexity) -> String {
    match complexity {
        Complexity::Simple => {
            "完成后请调用寸止(zhi)工具获取用户反馈".to_string()
        }
        Complexity::Medium => {
            "建议每步完成后调用寸止(zhi)确认，确保方向正确".to_string()
        }
        Complexity::Complex => {
            "请先确认方案再实施，每个关键节点调用寸止(zhi)确认".to_string()
        }
    }
}

pub fn check_workflow(
    definition: &WorkflowDefinition,
    task_description: &str,
    hint_complexity: Option<&str>,
    completed_steps: &[String],
) -> WorkflowCheckResult {
    let hint = evaluate_workflow(definition, task_description, hint_complexity);

    let mut missing_steps = Vec::new();

    for step in &hint.suggested_steps {
        if !completed_steps.iter().any(|c| c == &step.id) {
            missing_steps.push(MissingStep {
                id: step.id.clone(),
                name: step.name.clone(),
                action: step.action.clone(),
            });
        }
    }

    let passed = missing_steps.is_empty();

    let message = if passed {
        "✅ 所有建议步骤已完成，工作流执行正确".to_string()
    } else {
        let names: Vec<&str> = missing_steps.iter().map(|s| s.name.as_str()).collect();
        format!("⚠️ 以下步骤尚未完成: {}", names.join("、"))
    };

    let progress_display = build_progress_display(&hint.suggested_steps, completed_steps);

    WorkflowCheckResult {
        passed,
        missing_steps,
        completed_steps: completed_steps.to_vec(),
        loop_info: hint.loop_info,
        message,
        progress_display,
    }
}

fn build_progress_display(steps: &[SuggestedStep], completed: &[String]) -> String {
    let mut lines = Vec::new();
    for step in steps {
        let done = completed.iter().any(|c| c == &step.id);
        let checkbox = if done { "- [x]" } else { "- [ ]" };
        lines.push(format!("{} {}", checkbox, step.name));
    }
    lines.join("\n")
}
