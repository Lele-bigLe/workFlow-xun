// 工作流引擎
// 根据任务描述评估复杂度，生成工作流建议路径

use super::definition::*;
use sha2::{Digest, Sha256};
use std::collections::HashSet;

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
    let expected_step_ids: Vec<String> = suggested_steps.iter().map(|step| step.id.clone()).collect();
    let workflow_fingerprint = build_workflow_fingerprint(definition);
    let complexity_text = complexity.to_string();
    let hint_fingerprint = build_hint_fingerprint(
        task_description,
        &complexity_text,
        &expected_step_ids,
        &workflow_fingerprint,
    );

    // 构建循环回退信息
    let loop_info = build_loop_info(&definition.nodes, &suggested_steps);

    WorkflowHintResult {
        contract_version: WORKFLOW_CONTRACT_VERSION,
        complexity: complexity_text,
        workflow_fingerprint,
        hint_fingerprint,
        expected_step_ids,
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
    expected_steps: Option<&[SuggestedStep]>,
    workflow_fingerprint: Option<&str>,
    hint_fingerprint: Option<&str>,
) -> WorkflowCheckResult {
    let live_hint = evaluate_workflow(definition, task_description, hint_complexity);
    let current_workflow_fingerprint = live_hint.workflow_fingerprint.clone();
    let provided_workflow_fingerprint = workflow_fingerprint.map(str::to_string);
    let snapshot_steps = match expected_steps {
        Some(steps) if !steps.is_empty() => steps.to_vec(),
        _ => live_hint.suggested_steps.clone(),
    };
    let expected_step_ids: Vec<String> = snapshot_steps.iter().map(|step| step.id.clone()).collect();
    let fingerprint_complexity = hint_complexity.unwrap_or(live_hint.complexity.as_str());
    let effective_workflow_fingerprint = workflow_fingerprint.unwrap_or(current_workflow_fingerprint.as_str());
    let computed_hint_fingerprint = build_hint_fingerprint(
        task_description,
        fingerprint_complexity,
        &expected_step_ids,
        effective_workflow_fingerprint,
    );
    let (normalized_completed_steps, duplicate_completed_steps) = normalize_completed_steps(completed_steps);
    let unknown_completed_steps = collect_unknown_completed_steps(&normalized_completed_steps, &expected_step_ids);
    let missing_steps = collect_missing_steps(&snapshot_steps, &normalized_completed_steps);
    let completion_rate = build_completion_rate(snapshot_steps.len(), missing_steps.len());
    let progress_display = build_progress_display(&snapshot_steps, &normalized_completed_steps);

    let mut diagnostics = Vec::new();
    if !duplicate_completed_steps.is_empty() {
        diagnostics.push(format!(
            "检测到重复 completed_steps: {}",
            duplicate_completed_steps.join("、")
        ));
    }
    if !unknown_completed_steps.is_empty() {
        diagnostics.push(format!(
            "检测到未出现在 hint 快照中的步骤: {}",
            unknown_completed_steps.join("、")
        ));
    }

    if let Some(provided_fingerprint) = workflow_fingerprint {
        if provided_fingerprint != current_workflow_fingerprint {
            diagnostics.push("当前 workflow.yaml 已变化，本次 check 不能继续沿用旧 hint 快照".to_string());
            return WorkflowCheckResult {
                contract_version: WORKFLOW_CONTRACT_VERSION,
                status: CheckStatus::StaleConfig,
                passed: false,
                should_rehint: true,
                missing_steps,
                missing_required_steps: Vec::new(),
                completed_steps: completed_steps.to_vec(),
                normalized_completed_steps,
                unknown_completed_steps,
                duplicate_completed_steps,
                expected_step_ids,
                current_workflow_fingerprint,
                provided_workflow_fingerprint,
                hint_fingerprint: computed_hint_fingerprint,
                completion_rate,
                diagnostics,
                loop_info: None,
                message: "⚠️ 当前工作流配置已变化，请重新调用 hint 获取新的步骤快照".to_string(),
                progress_display: progress_display.clone(),
            };
        }
    }

    let expected_matches_live = expected_steps
        .filter(|steps| !steps.is_empty())
        .map(|_| live_hint.expected_step_ids == expected_step_ids)
        .unwrap_or(true);

    if let Some(provided_hint_fingerprint) = hint_fingerprint {
        if provided_hint_fingerprint != computed_hint_fingerprint {
            diagnostics.push("hint_fingerprint 与当前 check 输入不匹配，hint 快照可能已损坏或字段未同步".to_string());
            return WorkflowCheckResult {
                contract_version: WORKFLOW_CONTRACT_VERSION,
                status: CheckStatus::InvalidHintSnapshot,
                passed: false,
                should_rehint: true,
                missing_steps,
                missing_required_steps: Vec::new(),
                completed_steps: completed_steps.to_vec(),
                normalized_completed_steps,
                unknown_completed_steps,
                duplicate_completed_steps,
                expected_step_ids,
                current_workflow_fingerprint,
                provided_workflow_fingerprint,
                hint_fingerprint: computed_hint_fingerprint,
                completion_rate,
                diagnostics,
                loop_info: None,
                message: "⚠️ hint 快照校验失败，请重新调用 hint 后再执行 check".to_string(),
                progress_display: progress_display.clone(),
            };
        }
    } else if !expected_matches_live {
        diagnostics.push("expected_steps 与当前 hint 展开结果不一致，且未提供 hint_fingerprint 作为快照校验锚点".to_string());
        return WorkflowCheckResult {
            contract_version: WORKFLOW_CONTRACT_VERSION,
            status: CheckStatus::InvalidHintSnapshot,
            passed: false,
            should_rehint: true,
            missing_steps,
            missing_required_steps: Vec::new(),
            completed_steps: completed_steps.to_vec(),
            normalized_completed_steps,
            unknown_completed_steps,
            duplicate_completed_steps,
            expected_step_ids,
            current_workflow_fingerprint,
            provided_workflow_fingerprint,
            hint_fingerprint: computed_hint_fingerprint,
            completion_rate,
            diagnostics,
            loop_info: None,
            message: "⚠️ hint 快照不完整或已漂移，请重新调用 hint 获取稳定锚点".to_string(),
            progress_display: progress_display.clone(),
        };
    }

    let missing_required_steps = collect_missing_required_steps(definition, &missing_steps);
    let status = if missing_steps.is_empty() {
        CheckStatus::Ok
    } else {
        CheckStatus::MissingSteps
    };
    let passed = matches!(status, CheckStatus::Ok);
    let message = if passed {
        "✅ hint 快照中的所有建议步骤已完成，工作流执行正确".to_string()
    } else {
        let names: Vec<&str> = missing_steps.iter().map(|step| step.name.as_str()).collect();
        format!("⚠️ 以下步骤尚未完成: {}", names.join("、"))
    };
    let loop_info = if live_hint.expected_step_ids == expected_step_ids {
        live_hint.loop_info
    } else {
        None
    };

    WorkflowCheckResult {
        contract_version: WORKFLOW_CONTRACT_VERSION,
        status,
        passed,
        should_rehint: false,
        missing_steps,
        missing_required_steps,
        completed_steps: completed_steps.to_vec(),
        normalized_completed_steps: normalized_completed_steps.clone(),
        unknown_completed_steps,
        duplicate_completed_steps,
        expected_step_ids,
        current_workflow_fingerprint,
        provided_workflow_fingerprint,
        hint_fingerprint: computed_hint_fingerprint,
        completion_rate,
        diagnostics,
        loop_info,
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

fn collect_missing_steps(steps: &[SuggestedStep], completed: &[String]) -> Vec<MissingStep> {
    steps.iter()
        .filter(|step| !completed.iter().any(|done| done == &step.id))
        .map(|step| MissingStep {
            id: step.id.clone(),
            name: step.name.clone(),
            action: step.action.clone(),
        })
        .collect()
}

fn collect_missing_required_steps(
    definition: &WorkflowDefinition,
    missing_steps: &[MissingStep],
) -> Vec<MissingStep> {
    missing_steps.iter()
        .filter(|step| {
            definition.nodes.iter().any(|node| node.id == step.id && node.required)
        })
        .cloned()
        .collect()
}

fn collect_unknown_completed_steps(completed: &[String], expected_step_ids: &[String]) -> Vec<String> {
    let expected: HashSet<&str> = expected_step_ids.iter().map(String::as_str).collect();
    completed.iter()
        .filter(|step_id| !expected.contains(step_id.as_str()))
        .cloned()
        .collect()
}

fn normalize_completed_steps(completed_steps: &[String]) -> (Vec<String>, Vec<String>) {
    let mut seen = HashSet::new();
    let mut duplicate_seen = HashSet::new();
    let mut normalized = Vec::new();
    let mut duplicates = Vec::new();

    for step in completed_steps {
        let normalized_step = step.trim();
        if normalized_step.is_empty() {
            continue;
        }

        if seen.insert(normalized_step.to_string()) {
            normalized.push(normalized_step.to_string());
        } else if duplicate_seen.insert(normalized_step.to_string()) {
            duplicates.push(normalized_step.to_string());
        }
    }

    (normalized, duplicates)
}

fn build_completion_rate(total_steps: usize, missing_steps: usize) -> f32 {
    if total_steps == 0 {
        return 1.0;
    }

    (total_steps.saturating_sub(missing_steps) as f32) / total_steps as f32
}

pub fn build_workflow_fingerprint(definition: &WorkflowDefinition) -> String {
    let mut lines = Vec::new();

    for node in &definition.nodes {
        lines.push(format!(
            "node|{}|{}|{}|{}|{}|{}",
            node.id,
            node.name,
            node.required,
            node.action,
            node.loop_back_to.as_deref().unwrap_or(""),
            node.skip_when.join("\u{001f}")
        ));
    }

    let mut rules: Vec<_> = definition.complexity_rules.iter().collect();
    rules.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (level, rule) in rules {
        lines.push(format!(
            "rule|{}|{}|{}",
            level,
            rule.max_files.map(|value| value.to_string()).unwrap_or_default(),
            rule.nature.join("\u{001f}")
        ));
    }

    hash_text(&lines.join("\n"))
}

fn build_hint_fingerprint(
    task_description: &str,
    complexity: &str,
    expected_step_ids: &[String],
    workflow_fingerprint: &str,
) -> String {
    let mut lines = vec![
        normalize_task_description(task_description),
        complexity.trim().to_lowercase(),
        workflow_fingerprint.to_string(),
    ];
    lines.extend(expected_step_ids.iter().cloned());
    hash_text(&lines.join("\n"))
}

fn normalize_task_description(task_description: &str) -> String {
    task_description.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn hash_text(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
