#![allow(dead_code)]

#[cfg(feature = "gui")]
use workflow::definition::{WorkflowConfigMetadata, WorkflowDefinition};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[cfg(feature = "gui")]
#[tauri::command]
fn get_workflow_config() -> Result<WorkflowDefinition, String> {
    let path = workflow::loader::get_workflow_config_path()
        .ok_or("无法确定配置保存路径")?;
    
    if !path.exists() {
        // 返回系统内置的默认工作流模板
        return Ok(workflow::default::default_workflow());
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("读取配置失败: {}", e))?;
        
    let definition: WorkflowDefinition = serde_yaml::from_str(&content)
        .map_err(|e| format!("YAML 解析配置失败: {}", e))?;
        
    Ok(definition)
}

#[cfg(feature = "gui")]
#[tauri::command]
fn save_workflow_config(config: WorkflowDefinition) -> Result<(), String> {
    workflow::loader::ensure_valid_workflow_definition(&config)?;

    let path = workflow::loader::get_workflow_config_path()
        .ok_or("无法确定配置保存路径")?;
    
    // 确保父目录存在
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // 保存历史快照（静默，不阻断主流程）
    workflow::loader::save_history_snapshot(&config);

    let content = serde_yaml::to_string(&config)
        .map_err(|e| format!("YAML 序列化失败: {}", e))?;

    std::fs::write(&path, content)
        .map_err(|e| format!("配置文件写入失败: {}", e))
}

#[cfg(feature = "gui")]
#[tauri::command]
fn get_workflow_config_metadata() -> Result<WorkflowConfigMetadata, String> {
    let definition = get_workflow_config()?;
    Ok(workflow::loader::build_workflow_config_metadata(&definition))
}

#[cfg(feature = "gui")]
#[tauri::command]
fn inspect_workflow_config(config: WorkflowDefinition) -> Result<WorkflowConfigMetadata, String> {
    Ok(workflow::loader::build_workflow_config_metadata(&config))
}

#[cfg(feature = "gui")]
#[tauri::command]
fn get_system_default_workflow() -> Result<WorkflowDefinition, String> {
    Ok(workflow::default::default_workflow())
}

#[cfg(feature = "gui")]
#[tauri::command]
fn get_workflow_presets() -> Result<Vec<workflow::definition::WorkflowPreset>, String> {
    workflow::loader::ensure_presets_initialized();
    Ok(workflow::loader::load_custom_presets())
}

#[cfg(feature = "gui")]
#[tauri::command]
fn save_custom_preset(preset: workflow::definition::WorkflowPreset) -> Result<(), String> {
    workflow::loader::save_custom_preset(preset)
}

#[cfg(feature = "gui")]
#[tauri::command]
fn delete_custom_preset(name: String) -> Result<(), String> {
    workflow::loader::delete_custom_preset(&name)
}

#[cfg(feature = "gui")]
#[tauri::command]
fn rename_custom_preset(old_name: String, new_name: String) -> Result<(), String> {
    workflow::loader::rename_custom_preset(&old_name, &new_name)
}

#[cfg(feature = "gui")]
#[tauri::command]
fn yaml_to_json(yaml: String) -> Result<String, String> {
    let val: serde_yaml::Value = serde_yaml::from_str(&yaml).map_err(|e| format!("解析 YAML 失败: {}", e))?;
    serde_json::to_string(&val).map_err(|e| format!("序列化 JSON 失败: {}", e))
}

#[cfg(feature = "gui")]
#[tauri::command]
fn json_to_yaml(json: String) -> Result<String, String> {
    let val: serde_json::Value = serde_json::from_str(&json).map_err(|e| format!("解析 JSON 失败: {}", e))?;
    serde_yaml::to_string(&val).map_err(|e| format!("序列化 YAML 失败: {}", e))
}

#[cfg(feature = "gui")]
#[tauri::command]
fn write_export_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content)
        .map_err(|e| format!("文件写入失败: {}", e))
}

#[cfg(feature = "gui")]
#[tauri::command]
fn get_data_dir() -> String {
    workflow::loader::get_data_dir_display()
}

#[cfg(feature = "gui")]
#[tauri::command]
fn set_data_dir(dir: String) -> Result<(), String> {
    workflow::loader::set_data_dir(&dir)
}

pub mod workflow;
pub mod workflow_mcp;

#[cfg(feature = "gui")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            get_workflow_config, 
            get_workflow_config_metadata,
            inspect_workflow_config,
            save_workflow_config, 
            get_system_default_workflow, 
            get_workflow_presets,
            save_custom_preset,
            delete_custom_preset,
            rename_custom_preset,
            yaml_to_json,
            json_to_yaml,
            write_export_file,
            get_data_dir,
            set_data_dir
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
