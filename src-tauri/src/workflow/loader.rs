// 工作流配置加载器
// 支持从 ~/.xun/workflow.yaml 加载自定义配置

use anyhow::Result;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use super::definition::{
    WorkflowConfigMetadata, WorkflowDefinition, WorkflowValidationIssue,
    WorkflowValidationSeverity,
};
use super::default::default_workflow;

#[derive(Clone)]
struct WorkflowCacheEntry {
    path_key: Option<String>,
    modified_at: Option<SystemTime>,
    definition: WorkflowDefinition,
}

fn workflow_cache() -> &'static Mutex<Option<WorkflowCacheEntry>> {
    static CACHE: OnceLock<Mutex<Option<WorkflowCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn workflow_modified_at(path: &PathBuf) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

/// 用户设置（存储在固定位置 ~/.xun/settings.yaml）
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Settings {
    /// 自定义数据目录路径（为空时使用默认 ~/.xun/）
    data_dir: Option<String>,
}

/// 获取设置文件路径（始终在 ~/.xun/settings.yaml）
fn get_settings_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".xun").join("settings.yaml"))
}

/// 读取设置
fn load_settings() -> Settings {
    let path = match get_settings_path() {
        Some(p) => p,
        None => return Settings::default(),
    };
    if let Ok(content) = std::fs::read_to_string(&path) {
        serde_yaml::from_str(&content).unwrap_or_default()
    } else {
        Settings::default()
    }
}

/// 获取数据根目录（优先自定义，否则 ~/.xun/）
pub fn get_base_data_dir() -> Option<PathBuf> {
    let settings = load_settings();
    if let Some(ref dir) = settings.data_dir {
        let p = PathBuf::from(dir);
        if !p.as_os_str().is_empty() {
            return Some(p);
        }
    }
    dirs::home_dir().map(|h| h.join(".xun"))
}

/// 获取当前数据目录（供前端显示）
pub fn get_data_dir_display() -> String {
    get_base_data_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 设置自定义数据目录
pub fn set_data_dir(dir: &str) -> std::result::Result<(), String> {
    let settings_path = get_settings_path().ok_or("无法确定设置文件路径")?;
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut settings = load_settings();
    settings.data_dir = if dir.is_empty() { None } else { Some(dir.to_string()) };
    let content = serde_yaml::to_string(&settings).map_err(|e| e.to_string())?;
    std::fs::write(&settings_path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn load_workflow_definition() -> WorkflowDefinition {
    let config_path = get_workflow_config_path();
    let path_key = config_path.as_ref().map(|path| path.to_string_lossy().to_string());
    let modified_at = config_path.as_ref().and_then(workflow_modified_at);

    if let Ok(cache) = workflow_cache().lock() {
        if let Some(entry) = cache.as_ref() {
            if entry.path_key == path_key && entry.modified_at == modified_at {
                return entry.definition.clone();
            }
        }
    }

    let definition = match try_load_custom_workflow() {
        Ok(Some(def)) => {
            log::info!("已加载自定义工作流配置");
            def
        }
        Ok(None) => {
            log::debug!("未找到自定义工作流配置，使用内置默认值");
            default_workflow()
        }
        Err(e) => {
            log::warn!("加载自定义工作流配置失败: {}，使用内置默认值", e);
            default_workflow()
        }
    };

    if let Ok(mut cache) = workflow_cache().lock() {
        *cache = Some(WorkflowCacheEntry {
            path_key,
            modified_at,
            definition: definition.clone(),
        });
    }

    definition
}

pub fn get_workflow_config_path() -> Option<PathBuf> {
    // 优先级 0：用户自定义数据目录（settings.yaml 中配置）
    if let Some(base) = get_base_data_dir() {
        let custom_config = base.join("workflow.yaml");
        // 自定义目录已存在配置，或目录已经被用户指定过（即使文件还不存在也用它）
        let settings = load_settings();
        if settings.data_dir.is_some() {
            return Some(custom_config);
        }
    }

    // 优先级 1：优先尝试读取可执行文件同目录下的 workflow.yaml
    if let Ok(mut exe_path) = std::env::current_exe() {
        exe_path.pop();
        let local_config = exe_path.join("workflow.yaml");
        if local_config.exists() {
            return Some(local_config);
        }
    }

    // 优先级 2：当前工作目录下的 workflow.yaml (用于开发调试)
    let cwd_config = std::env::current_dir().unwrap_or_default().join("workflow.yaml");
    if cwd_config.exists() {
        return Some(cwd_config);
    }

    // 优先级 3：全局用户目录 ~/.xun/workflow.yaml（兜底返回）
    if let Some(home) = dirs::home_dir() {
        return Some(home.join(".xun").join("workflow.yaml"));
    }

    None
}

fn try_load_custom_workflow() -> Result<Option<WorkflowDefinition>> {
    let config_path_opt = get_workflow_config_path();

    let config_path = match config_path_opt {
        Some(path) => path,
        None => return Ok(None),
    };

    let content = std::fs::read_to_string(&config_path)?;
    let definition: WorkflowDefinition = serde_yaml::from_str(&content)?;

    let issues = validate_workflow_definition(&definition);
    let blocking_issues: Vec<&WorkflowValidationIssue> = issues.iter()
        .filter(|issue| matches!(issue.severity, WorkflowValidationSeverity::Error))
        .collect();
    if !blocking_issues.is_empty() {
        let messages: Vec<&str> = blocking_issues.iter().map(|issue| issue.message.as_str()).collect();
        anyhow::bail!("workflow.yaml 配置校验失败: {}", messages.join("；"));
    }

    Ok(Some(definition))
}

pub fn validate_workflow_definition(definition: &WorkflowDefinition) -> Vec<WorkflowValidationIssue> {
    let mut issues = Vec::new();

    if definition.nodes.is_empty() {
        issues.push(WorkflowValidationIssue {
            code: "workflow_empty".to_string(),
            severity: WorkflowValidationSeverity::Error,
            node_id: None,
            message: "workflow.yaml 中节点列表为空".to_string(),
        });
        return issues;
    }

    let mut seen_ids = HashSet::new();
    let mut duplicate_ids = HashSet::new();
    let mut valid_node_ids = HashSet::new();

    for node in &definition.nodes {
        let node_id = node.id.trim();

        if node_id.is_empty() {
            issues.push(WorkflowValidationIssue {
                code: "empty_node_id".to_string(),
                severity: WorkflowValidationSeverity::Error,
                node_id: None,
                message: format!("节点「{}」缺少 id", node.name),
            });
        } else if !seen_ids.insert(node_id.to_string()) {
            duplicate_ids.insert(node_id.to_string());
        } else {
            valid_node_ids.insert(node_id.to_string());
        }

        if node.name.trim().is_empty() {
            issues.push(WorkflowValidationIssue {
                code: "empty_node_name".to_string(),
                severity: WorkflowValidationSeverity::Error,
                node_id: (!node_id.is_empty()).then(|| node_id.to_string()),
                message: format!("节点 id={} 缺少 name", if node_id.is_empty() { "<empty>" } else { node_id }),
            });
        }

        if node.required && !node.skip_when.is_empty() {
            issues.push(WorkflowValidationIssue {
                code: "required_node_has_skip_when".to_string(),
                severity: WorkflowValidationSeverity::Warning,
                node_id: (!node_id.is_empty()).then(|| node_id.to_string()),
                message: format!("节点「{}」被标记为 required，skip_when 将被运行时忽略", node.name),
            });
        }
    }

    let mut duplicate_ids: Vec<String> = duplicate_ids.into_iter().collect();
    duplicate_ids.sort();
    for duplicate_id in duplicate_ids {
        issues.push(WorkflowValidationIssue {
            code: "duplicate_node_id".to_string(),
            severity: WorkflowValidationSeverity::Error,
            node_id: Some(duplicate_id.clone()),
            message: format!("节点 id「{}」重复，check/hint 无法稳定追踪步骤", duplicate_id),
        });
    }

    for node in &definition.nodes {
        let node_id = node.id.trim();
        if let Some(target) = node.loop_back_to.as_deref() {
            let target = target.trim();
            if target.is_empty() {
                issues.push(WorkflowValidationIssue {
                    code: "empty_loop_back_to".to_string(),
                    severity: WorkflowValidationSeverity::Error,
                    node_id: (!node_id.is_empty()).then(|| node_id.to_string()),
                    message: format!("节点「{}」配置了空的 loop_back_to", node.name),
                });
            } else if !valid_node_ids.contains(target) {
                issues.push(WorkflowValidationIssue {
                    code: "invalid_loop_back_to".to_string(),
                    severity: WorkflowValidationSeverity::Error,
                    node_id: (!node_id.is_empty()).then(|| node_id.to_string()),
                    message: format!("节点「{}」的 loop_back_to 指向不存在的节点 id「{}」", node.name, target),
                });
            }
        }
    }

    issues
}

pub fn build_workflow_config_metadata(definition: &WorkflowDefinition) -> WorkflowConfigMetadata {
    let issues = validate_workflow_definition(definition);
    let has_errors = issues.iter()
        .any(|issue| matches!(issue.severity, WorkflowValidationSeverity::Error));

    WorkflowConfigMetadata {
        workflow_fingerprint: super::engine::build_workflow_fingerprint(definition),
        config_path: get_workflow_config_path().map(|path| path.to_string_lossy().to_string()),
        has_errors,
        issues,
    }
}

pub fn ensure_valid_workflow_definition(definition: &WorkflowDefinition) -> Result<(), String> {
    let issues = validate_workflow_definition(definition);
    let blocking_issues: Vec<&WorkflowValidationIssue> = issues.iter()
        .filter(|issue| matches!(issue.severity, WorkflowValidationSeverity::Error))
        .collect();

    if blocking_issues.is_empty() {
        return Ok(());
    }

    let messages: Vec<&str> = blocking_issues.iter().map(|issue| issue.message.as_str()).collect();
    Err(format!("配置校验失败: {}", messages.join("；")))
}

pub fn generate_workflow_rules_text(definition: &WorkflowDefinition) -> String {
    let mut text = String::new();
    text.push_str("# 工作流规则\n\n");
    text.push_str("## 执行节点\n\n");

    for node in &definition.nodes {
        let required_tag = if node.required { " [必需]" } else { "" };
        text.push_str(&format!("- **{}**{}: {}\n", node.name, required_tag, node.action));

        if !node.skip_when.is_empty() {
            text.push_str(&format!("  跳过条件: {}\n", node.skip_when.join(", ")));
        }
    }

    text.push_str("\n## 复杂度规则\n\n");

    for (level, rule) in &definition.complexity_rules {
        text.push_str(&format!("### {}\n", level));
        if let Some(max) = rule.max_files {
            text.push_str(&format!("- 最大文件数: {}\n", max));
        }
        if !rule.nature.is_empty() {
            text.push_str(&format!("- 触发关键词: {}\n", rule.nature.join(", ")));
        }
        text.push('\n');
    }

    text.push_str("## 核心要求\n\n");
    text.push_str("1. **必须**在任务开始时调用 `hint` 工具获取工作流建议\n");
    text.push_str("2. 按 `suggested_steps` 列表逐步执行\n");
    text.push_str("3. 完成后**必须**调用寸止(zhi)工具获取用户反馈\n");
    text.push_str("4. 未收到\"结束\"指令前，禁止自行结束交互\n\n");
    text.push_str("## 自定义跳过条件（AI 自主判研）\n\n");
    text.push_str("当 `hint` 返回的 `suggested_steps` 中包含 `skip_conditions` 字段时，\n");
    text.push_str("这些是用户自定义的白话跳过条件。你需要根据当前任务描述，\n");
    text.push_str("自主判断是否满足这些条件。若满足，可合理跳过该步骤。\n\n");

    // hint 返回字段说明
    text.push_str("## hint 返回字段说明\n\n");
    text.push_str("| 字段 | 类型 | 说明 |\n");
    text.push_str("|------|------|------|\n");
    text.push_str("| `complexity` | string | 任务复杂度（simple/medium/complex），决定执行深度和必要步骤 |\n");
    text.push_str("| `suggested_steps` | array | 建议执行的步骤列表，按顺序逐步执行。每项含 `id`（步骤标识）、`name`（显示名）、`action`（具体执行指令）、`skip_conditions`（AI 自主判断的白话跳过条件） |\n");
    text.push_str("| `skipped_steps` | array | 已被引擎跳过的步骤，含 `id` 和 `reason`（跳过原因），无需执行 |\n");
    text.push_str("| `loop_info` | object/null | 循环回退信息。`loop_node_id`=触发循环的节点、`loop_back_to`=回退目标节点、`re_execute_nodes`=循环时需重新执行的节点列表。为 null 表示无循环 |\n");
    text.push_str("| `reminder` | string | 根据复杂度生成的执行提醒，建议遵循 |\n");
    text.push_str("| `progress_display` | string | Markdown 格式的进度清单（checkbox 列表），可直接展示给用户 |\n\n");

    // check 返回字段说明
    text.push_str("## check 返回字段说明\n\n");
    text.push_str("| 字段 | 类型 | 说明 |\n");
    text.push_str("|------|------|------|\n");
    text.push_str("| `passed` | boolean | 是否通过检查（true=所有建议步骤已完成） |\n");
    text.push_str("| `missing_steps` | array | 尚未完成的步骤列表，每项含 `id`、`name`、`action` |\n");
    text.push_str("| `completed_steps` | array | 已完成的步骤 ID 列表（即传入参数的回显） |\n");
    text.push_str("| `loop_info` | object/null | 与 hint 相同的循环回退信息 |\n");
    text.push_str("| `message` | string | 检查结果的文字摘要（✅ 通过 或 ⚠️ 遗漏提示） |\n");
    text.push_str("| `progress_display` | string | 带完成状态的 Markdown checkbox 进度清单 |\n");

    text
}

pub fn get_presets_file_path() -> Option<PathBuf> {
    get_workflow_config_path().map(|mut p| {
        p.set_file_name("presets.yaml");
        p
    })
}

/// 确保预设文件已初始化：首次启动时将内置预设播种到自定义文件
pub fn ensure_presets_initialized() {
    let path = match get_presets_file_path() {
        Some(p) => p,
        None => return,
    };
    if !path.exists() {
        let builtins = super::default::get_presets();
        if let Ok(content) = serde_yaml::to_string(&builtins) {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&path, content);
        }
    }
}

pub fn load_custom_presets() -> Vec<super::definition::WorkflowPreset> {
    let path_opt = get_presets_file_path();
    let path = match path_opt {
        Some(p) => p,
        None => return vec![],
    };
    if let Ok(content) = std::fs::read_to_string(&path) {
        if let Ok(presets) = serde_yaml::from_str(&content) {
            return presets;
        }
    }
    vec![]
}

pub fn save_custom_preset(preset: super::definition::WorkflowPreset) -> Result<(), String> {
    ensure_valid_workflow_definition(&preset.workflow)?;

    let path = get_presets_file_path().ok_or("无法获取预设文件路径")?;
    let mut presets = load_custom_presets();
    
    // 如果有同名的预设，就直接覆盖更新
    if let Some(existing) = presets.iter_mut().find(|p| p.name == preset.name) {
        *existing = preset;
    } else {
        presets.push(preset);
    }
    
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    
    let content = serde_yaml::to_string(&presets).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_custom_preset(name: &str) -> Result<(), String> {
    let path = get_presets_file_path().ok_or("无法获取预设文件路径")?;
    let mut presets = load_custom_presets();
    presets.retain(|p| p.name != name);
    
    let content = serde_yaml::to_string(&presets).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn rename_custom_preset(old_name: &str, new_name: &str) -> Result<(), String> {
    let path = get_presets_file_path().ok_or("无法获取预设文件路径")?;
    let mut presets = load_custom_presets();
    
    if presets.iter().any(|p| p.name == new_name) {
        return Err(format!("预设名「{}」已存在", new_name));
    }
    if let Some(preset) = presets.iter_mut().find(|p| p.name == old_name) {
        preset.name = new_name.to_string();
    } else {
        return Err(format!("预设「{}」不存在", old_name));
    }
    
    let content = serde_yaml::to_string(&presets).map_err(|e| e.to_string())?;
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(())
}

/// 保存历史快照，最多保留最近 10 个
pub fn save_history_snapshot(config: &super::definition::WorkflowDefinition) {
    let base_path = match get_workflow_config_path() {
        Some(p) => p,
        None => return,
    };
    let history_dir = match base_path.parent() {
        Some(parent) => parent.join("history"),
        None => return,
    };
    let _ = std::fs::create_dir_all(&history_dir);

    // 用 UNIX 时间戳生成唯一文件名
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let snapshot_path = history_dir.join(format!("workflow_{}.yaml", ts));

    if let Ok(content) = serde_yaml::to_string(config) {
        let _ = std::fs::write(&snapshot_path, content);
    }

    // 清理：只保留最近 10 个
    cleanup_old_snapshots(&history_dir, 10);
}

fn cleanup_old_snapshots(history_dir: &std::path::Path, max_count: usize) {
    let mut entries: Vec<_> = std::fs::read_dir(history_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "yaml")
                .unwrap_or(false)
        })
        .collect();

    entries.sort_by_key(|e| e.file_name());

    if entries.len() > max_count {
        for entry in &entries[..entries.len() - max_count] {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
