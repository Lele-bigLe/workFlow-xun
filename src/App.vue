<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";

// 数据模型
interface WorkflowNode {
  id: string;
  name: string;
  required: boolean;
  skip_when: string[];
  action: string;
  loop_back_to?: string | null;
}

interface ComplexityRule {
  max_files: number | null;
  nature: string[];
}

interface WorkflowDefinition {
  nodes: WorkflowNode[];
  complexity_rules: Record<string, ComplexityRule>;
}

interface WorkflowPreset {
  name: string;
  description: string;
  workflow: WorkflowDefinition;
}

const config = ref<WorkflowDefinition>({
  nodes: [],
  complexity_rules: {},
});

// 稳定的节点 UID，不污染数据模型，避免编辑 node.id 时 key 变化导致 DOM 重建
let _uidCounter = 0;
const _nodeUids = new WeakMap<object, number>();
function nodeKey(node: object): number {
  if (!_nodeUids.has(node)) _nodeUids.set(node, ++_uidCounter);
  return _nodeUids.get(node)!;
}

const presets = ref<WorkflowPreset[]>([]);

const isSaving = ref(false);
const isLoading = ref(false);
const saveMessage = ref("");
const notifTimer = ref<number | null>(null);
const activePresetName = ref<string | null>(null);

function notify(msg: string, duration = 3000) {
  saveMessage.value = msg;
  if (notifTimer.value) clearTimeout(notifTimer.value);
  if (duration > 0) {
    notifTimer.value = window.setTimeout(() => {
      saveMessage.value = "";
    }, duration);
  }
}

// 模态框态
const showPresetModal = ref(false);
const newPresetName = ref("");
const newPresetDesc = ref("");

// 通用确认模态框
const confirmModal = ref({
  show: false,
  title: "",
  message: "",
  onConfirm: () => {}
});

function showConfirm(title: string, message: string, onConfirm: () => void) {
  confirmModal.value = { show: true, title, message, onConfirm };
}

function executeConfirm() {
  confirmModal.value.onConfirm();
  confirmModal.value.show = false;
}

const selectedNodeIndex = ref<number | null>(null);
const mainPanelRef = ref<HTMLElement | null>(null);

// 拖拽态
const draggedIndex = ref<number | null>(null);
const dragOverIndex = ref<number | null>(null);

// 主题切换逻辑
const isDarkMode = ref(false);
function toggleTheme() {
  showPresetMenu.value = false;
  showExportMenu.value = false;
  isDarkMode.value = !isDarkMode.value;
  localStorage.setItem('xun-dark-mode', isDarkMode.value ? 'true' : 'false');
}

// 预设下拉菜单状态
const showPresetMenu = ref(false);
function togglePresetMenu() {
  showPresetMenu.value = !showPresetMenu.value;
  showExportMenu.value = false;
  showSettingsMenu.value = false;
  showPromptPanel.value = false;
}

// 设置面板状态
const showSettingsMenu = ref(false);
const currentDataDir = ref("");
function toggleSettingsMenu() {
  showSettingsMenu.value = !showSettingsMenu.value;
  showPresetMenu.value = false;
  showExportMenu.value = false;
  showPromptPanel.value = false;
  if (showSettingsMenu.value) {
    loadDataDir();
  }
}
async function loadDataDir() {
  try {
    currentDataDir.value = await invoke<string>("get_data_dir");
  } catch (e) {
    currentDataDir.value = "未知";
  }
}
async function chooseDataDir() {
  const selected = await open({ directory: true, title: "选择 xun 数据目录" });
  if (selected && typeof selected === "string") {
    try {
      await invoke("set_data_dir", { dir: selected });
      currentDataDir.value = selected;
      notify("✅ 数据目录已更新，重启后完全生效");
    } catch (e) {
      notify(`设置失败: ${e}`);
    }
  }
}
async function resetDataDir() {
  try {
    await invoke("set_data_dir", { dir: "" });
    currentDataDir.value = await invoke<string>("get_data_dir");
    notify("已恢复默认数据目录");
  } catch (e) {
    notify(`重置失败: ${e}`);
  }
}

// 导出下拉菜单状态
const showExportMenu = ref(false);
function toggleExportMenu() {
  showExportMenu.value = !showExportMenu.value;
  showPresetMenu.value = false;
  showSettingsMenu.value = false;
  showPromptPanel.value = false;
}

// 使用提示词面板
const showPromptPanel = ref(false);
const promptCopied = ref(false);
const mcpUsagePrompt = `## Xun Workflow MCP 使用指南

在你的项目 \`.vscode/mcp.json\` 中添加：
\`\`\`json
{
  "servers": {
    "workFlow": {
      "type": "stdio",
      "command": "workFlow.exe 的完整路径"
    }
  }
}
\`\`\`

然后在 \`.vscode/\` 或用户级 prompts 中添加以下规则：

### 核心规则（复制到你的 instructions 文件中）
1. 每个任务开始前，必须调用 \`mcp_workFlow_hint\` 获取工作流建议
2. 按返回的 \`suggested_steps\` 逐步执行
3. 任务完成前，可调用 \`mcp_workFlow_check\` 自检
4. 代码完成后调用寸止(zhi)工具获取用户反馈，循环到用户说「结束」`;

function togglePromptPanel() {
  showPromptPanel.value = !showPromptPanel.value;
  showPresetMenu.value = false;
  showExportMenu.value = false;
  showSettingsMenu.value = false;
}
function copyPrompt() {
  navigator.clipboard.writeText(mcpUsagePrompt).then(() => {
    promptCopied.value = true;
    setTimeout(() => { promptCopied.value = false; }, 2000);
  });
}

// ====== 配置防呆白话翻译引擎 ======
function toPlainConfig(cfg: WorkflowDefinition) {
  return {
    "执行流程": cfg.nodes.map((n, idx) => ({
      "执行序号": idx + 1,
      "节点代号_不可重复": n.id,
      "节点显示名称": n.name || "未命名节点",
      "强行必做_写true或false": n.required || false,
      "AI的具体执行指令": n.action || "",
      "满足以下情况跳过此节点_填列表": n.skip_when && n.skip_when.length > 0 ? n.skip_when : [],
      "循环回退目标节点ID": n.loop_back_to || null
    })),
    "高阶复杂度判研规则": cfg.complexity_rules || {}
  };
}

function fromPlainConfig(plain: any): WorkflowDefinition {
  const nodesSource = plain["执行流程"] || plain.nodes || plain;
  const compRules = plain["高阶复杂度判研规则"] || plain.complexity_rules || {};
  
  const nodes = Array.isArray(nodesSource) ? nodesSource.map((n: any, i: number) => ({
    id: String(n["节点代号_不可重复"] || n.id || `node_${Date.now()}_${i}`),
    name: String(n["节点显示名称"] || n.name || "未命名"),
    required: !!(n["强行必做_写true或false"] ?? n.required ?? false),
    action: String(n["AI的具体执行指令"] || n.action || ""),
    skip_when: Array.isArray(n["满足以下情况跳过此节点_填列表"] || n.skip_when) 
      ? (n["满足以下情况跳过此节点_填列表"] || n.skip_when) 
      : [],
    loop_back_to: n["循环回退目标节点ID"] || n.loop_back_to || null
  })) : [];
  
  return { nodes, complexity_rules: compRules };
}

// 导出分流器（使用原生保存对话框选择路径）
async function exportConfig(type: 'plain-yaml' | 'json' | 'yaml') {
  showExportMenu.value = false;
  try {
    let content = "";
    let defaultName = "";
    let filterName = "";
    let extensions: string[] = [];
    if (type === 'plain-yaml') {
      const plainObj = toPlainConfig(config.value);
      content = await invoke<string>("json_to_yaml", { json: JSON.stringify(plainObj) });
      content = "# Xun Workflow - 极简白话便携配置文件\n# ------------------------------------\n# 这是一个无需任何编程知识即可自由修改的文件。\n# 你可以直接用文档编辑器修改其中的文字：\n#   - 注意不要删掉节点字段前面的冒号和缩进\n#   - true/false 代表 是/否\n\n" + content;
      defaultName = "xun-workflow-plain.yaml";
      filterName = "YAML 文件";
      extensions = ["yaml", "yml"];
    } else if (type === 'yaml') {
      content = await invoke<string>("json_to_yaml", { json: JSON.stringify(config.value) });
      defaultName = "xun-workflow-standard.yaml";
      filterName = "YAML 文件";
      extensions = ["yaml", "yml"];
    } else {
      content = JSON.stringify(config.value, null, 2);
      defaultName = "xun-workflow-standard.json";
      filterName = "JSON 文件";
      extensions = ["json"];
    }
    const filePath = await save({
      defaultPath: defaultName,
      filters: [{ name: filterName, extensions }]
    });
    if (!filePath) {
      notify("已取消导出");
      return;
    }
    await invoke("write_export_file", { path: filePath, content });
    notify("配置已成功导出⬇️！");
  } catch(e) {
    notify(`导出失败: ${e}`);
  }
}

// 原生拾取器导入
function triggerUpload() {
  showExportMenu.value = false;
  const input = document.createElement('input');
  input.type = 'file';
  input.accept = ".json,.yaml,.yml";
  input.onchange = async (e) => {
    const file = (e.target as HTMLInputElement).files?.[0];
    if (!file) return;
    try {
      notify("正在解析配置文件...", 0);
      const text = await file.text();
      let jsonObj;
      if (file.name.endsWith('.yaml') || file.name.endsWith('.yml')) {
        const jsonStr = await invoke<string>("yaml_to_json", { yaml: text });
        jsonObj = JSON.parse(jsonStr);
      } else {
        jsonObj = JSON.parse(text);
      }
      config.value = fromPlainConfig(jsonObj);
      if (config.value.nodes.length > 0) selectedNodeIndex.value = 0;
      else selectedNodeIndex.value = null;
      notify("配置已成功导入并重置画布✨！");
    } catch(err) {
      notify(`导入解析失败: ${err}`);
    }
  };
  input.click();
}

// 初始化加载
onMounted(async () => {
  const savedTheme = localStorage.getItem('xun-dark-mode');
  if (savedTheme !== null) {
    isDarkMode.value = savedTheme === 'true';
  }
  
  try {
    presets.value = await invoke<WorkflowPreset[]>("get_workflow_presets");
    const data = await invoke<WorkflowDefinition>("get_workflow_config");
    config.value = data;
    if (config.value.nodes.length > 0) {
      selectedNodeIndex.value = 0;
    }
  } catch (e) {
    notify(`加载失败: ${e}`);
  }
});

// 保存配置
async function saveConfig() {
  showPresetMenu.value = false;
  showExportMenu.value = false;
  isSaving.value = true;
  notify("保存中...", 0);
  try {
    await invoke("save_workflow_config", { config: config.value });
    notify("保存成功！");
  } catch (e) {
    notify(`保存失败: ${e}`);
  } finally {
    isSaving.value = false;
  }
}

async function loadPreset(preset: WorkflowPreset) {
  isLoading.value = true;
  // 深拷贝切断索引绑定污染
  config.value = JSON.parse(JSON.stringify(preset.workflow));
  activePresetName.value = preset.name;
  if (config.value.nodes.length > 0) {
    selectedNodeIndex.value = 0;
  } else {
    selectedNodeIndex.value = null;
  }
  showPresetMenu.value = false;
  // 热切换：加载预设后自动应用到 workflow.yaml
  try {
    await invoke("save_workflow_config", { config: config.value });
    notify("✨ 已切换到预设「" + preset.name + "」并自动应用");
  } catch (e) {
    notify(`预设加载成功但自动应用失败: ${e}`);
  } finally {
    isLoading.value = false;
  }
}

// 覆盖更新当前已加载的预设
async function updateActivePreset() {
  if (!activePresetName.value) return;
  isLoading.value = true;
  try {
    notify("更新预设中...", 0);
    await invoke("save_custom_preset", {
      preset: { name: activePresetName.value, description: "", workflow: config.value }
    });
    presets.value = await invoke<WorkflowPreset[]>("get_workflow_presets");
    notify("✅ 预设已更新");
  } catch (e) {
    notify(`更新预设失败: ${e}`);
  } finally {
    isLoading.value = false;
  }
}

function clearCanvas() {
  showPresetMenu.value = false;
  showExportMenu.value = false;
  showConfirm("创建空白模板", "确定要创建一个全新的空白模板吗？此操作会瞬间清空当前画布上的所有节点。", () => {
    config.value.nodes = [];
    selectedNodeIndex.value = null;
    activePresetName.value = null;
    notify("已为您新建空白模板！");
  });
}

function openPresetModal() {
  showPresetMenu.value = false;
  showExportMenu.value = false;
  newPresetName.value = "";
  newPresetDesc.value = "";
  showPresetModal.value = true;
}

async function confirmSavePreset() {
  const name = newPresetName.value.trim();
  if (!name) return;
  const desc = newPresetDesc.value.trim() || "用户自定义的独立预设";
  isLoading.value = true;
  try {
    notify("保存预设中...", 0);
    await invoke("save_custom_preset", { preset: { name: name, description: desc, workflow: config.value } });
    presets.value = await invoke<WorkflowPreset[]>("get_workflow_presets");
    showPresetModal.value = false;
    notify("🎉 已存入专属预设库！");
  } catch(e) {
    notify(`写入失败: ${e}`);
  } finally {
    isLoading.value = false;
  }
}

async function deletePreset(name: string, event: Event) {
  event.stopPropagation();
  showConfirm("删除预设", `确定要永久移除预设「${name}」吗？操作不可逆。`, async () => {
    try {
      await invoke("delete_custom_preset", { name });
      presets.value = await invoke<WorkflowPreset[]>("get_workflow_presets");
      notify("已移除指定预设");
    } catch(e) {
      notify(`移除失败: ${e}`);
    }
  });
}

// 预设重命名
const editingPresetName = ref<string | null>(null);
const editingPresetNewName = ref("");

function startRenamePreset(name: string, event: Event) {
  event.stopPropagation();
  editingPresetName.value = name;
  editingPresetNewName.value = name;
}
async function confirmRenamePreset(event: Event) {
  event.stopPropagation();
  const oldName = editingPresetName.value;
  const newName = editingPresetNewName.value.trim();
  if (!oldName || !newName || oldName === newName) {
    editingPresetName.value = null;
    return;
  }
  try {
    await invoke("rename_custom_preset", { oldName, newName });
    presets.value = await invoke<WorkflowPreset[]>("get_workflow_presets");
    if (activePresetName.value === oldName) {
      activePresetName.value = newName;
    }
    notify("预设已重命名");
  } catch (e) {
    notify(`重命名失败: ${e}`);
  }
  editingPresetName.value = null;
}
function cancelRenamePreset(event: Event) {
  event.stopPropagation();
  editingPresetName.value = null;
}

function selectNode(index: number) {
  selectedNodeIndex.value = index;
  mainPanelRef.value?.scrollTo({ top: 0, behavior: 'smooth' });
}

// 从中文 name 生成 snake_case id
function nameToId(name: string): string {
  return name.trim().toLowerCase().replace(/\s+/g, '_').replace(/[^a-z0-9_\u4e00-\u9fff]/g, '') || `node_${Date.now()}`;
}

// 更新节点 name 时自动同步 id
function onNodeNameInput(index: number) {
  const node = config.value.nodes[index];
  const oldId = node.id;
  const newId = nameToId(node.name);
  // 检查是否重复，重复则加后缀
  const ids = config.value.nodes.filter((_: WorkflowNode, i: number) => i !== index).map((n: WorkflowNode) => n.id);
  let finalId = newId;
  let suffix = 2;
  while (ids.includes(finalId)) {
    finalId = `${newId}_${suffix++}`;
  }
  node.id = finalId;
  // 同步更新其他节点的 loop_back_to 引用
  for (const n of config.value.nodes) {
    if (n.loop_back_to === oldId) n.loop_back_to = finalId;
  }
}

function addNode() {
  const baseName = "新节点";
  const baseId = nameToId(baseName);
  const ids = config.value.nodes.map((n: WorkflowNode) => n.id);
  let finalId = baseId;
  let suffix = 2;
  while (ids.includes(finalId)) {
    finalId = `${baseId}_${suffix++}`;
  }
  config.value.nodes.push({
    id: finalId,
    name: baseName,
    action: "",
    required: false,
    skip_when: [],
    loop_back_to: null,
  });
  selectedNodeIndex.value = config.value.nodes.length - 1;
}

function removeNode(index: number) {
  config.value.nodes.splice(index, 1);
  if (selectedNodeIndex.value === index) {
    selectedNodeIndex.value = null;
  } else if (selectedNodeIndex.value !== null && selectedNodeIndex.value > index) {
    selectedNodeIndex.value--;
  }
}

// 辅助函数
const addSkipCondition = () => {
  if (selectedNodeIndex.value !== null) {
    config.value.nodes[selectedNodeIndex.value].skip_when.push("");
  }
};
const removeSkipCondition = (idx: number) => {
  if (selectedNodeIndex.value !== null) {
    config.value.nodes[selectedNodeIndex.value].skip_when.splice(idx, 1);
  }
};

// 拖拽逻辑
function onDragStart(event: DragEvent, index: number) {
  draggedIndex.value = index;
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", index.toString());
  }
}

function onDragEnter(index: number) {
  if (draggedIndex.value !== null && draggedIndex.value !== index) {
    dragOverIndex.value = index;
  }
}

function onDrop(index: number) {
  if (draggedIndex.value !== null && draggedIndex.value !== index) {
    // 移动数组元素
    const draggedItem = config.value.nodes.splice(draggedIndex.value, 1)[0];
    config.value.nodes.splice(index, 0, draggedItem);
    
    // 修正选中的焦点
    if (selectedNodeIndex.value === draggedIndex.value) {
      selectedNodeIndex.value = index;
    } else if (
      selectedNodeIndex.value !== null &&
      selectedNodeIndex.value > draggedIndex.value &&
      selectedNodeIndex.value <= index
    ) {
      selectedNodeIndex.value--;
    } else if (
      selectedNodeIndex.value !== null &&
      selectedNodeIndex.value < draggedIndex.value &&
      selectedNodeIndex.value >= index
    ) {
      selectedNodeIndex.value++;
    }
  }
  draggedIndex.value = null;
  dragOverIndex.value = null;
}

function onDragEnd() {
  draggedIndex.value = null;
  dragOverIndex.value = null;
}
</script>

<template>
  <main :class="{'dark': isDarkMode}" class="h-screen overflow-hidden w-full font-sans transition-colors duration-300">
    <div class="h-full w-full bg-neutral-50 dark:bg-neutral-950 text-neutral-900 dark:text-neutral-100 flex flex-col transition-colors duration-300 selection:bg-primary-500/30">
      <!-- 弹框等全局透明遮罩层（用于点击域外关闭） -->
      <div v-if="showPresetMenu || showExportMenu || showSettingsMenu || showPromptPanel" @click="showPresetMenu = false; showExportMenu = false; showSettingsMenu = false; showPromptPanel = false" class="fixed inset-0 z-40"></div>
    
      <!-- Toast Notifications (全局胶囊气泡悬浮提示弹窗) -->
    <transition
      enter-active-class="transition duration-300 ease-out transform"
      enter-from-class="-translate-y-4 opacity-0 scale-95"
      enter-to-class="translate-y-0 opacity-100 scale-100"
      leave-active-class="transition duration-200 ease-in transform"
      leave-from-class="opacity-100"
      leave-to-class="-translate-y-4 opacity-0 scale-95"
    >
      <div v-if="saveMessage" class="fixed top-20 left-1/2 -translate-x-1/2 z-[200] flex items-center gap-2 px-5 py-2.5 rounded-full shadow-xl border backdrop-blur-md whitespace-nowrap"
           :class="{
              'bg-red-50/90 dark:bg-red-950/90 border-red-200 dark:border-red-800/50 text-red-600 dark:text-red-400': saveMessage.includes('失败'),
              'bg-blue-50/90 dark:bg-blue-950/90 border-blue-200 dark:border-blue-800/50 text-blue-600 dark:text-blue-400': saveMessage.includes('保存中'),
              'bg-emerald-50/90 dark:bg-emerald-950/90 border-emerald-200 dark:border-emerald-800/50 text-emerald-600 dark:text-emerald-400': !saveMessage.includes('失败') && !saveMessage.includes('保存中')
           }">
        <svg v-if="saveMessage.includes('失败')" class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path></svg>
        <svg v-else-if="saveMessage.includes('保存中')" class="w-4 h-4 shrink-0 animate-spin" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path></svg>
        <svg v-else class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
        <span class="text-sm font-medium">{{ saveMessage }}</span>
      </div>
    </transition>

    <!-- Header -->
    <header class="relative h-16 shrink-0 border-b border-neutral-200 dark:border-neutral-800 bg-white dark:bg-neutral-900 flex items-center justify-between px-6">
      <div class="flex items-center gap-3 shrink-0 pr-4">
        <div class="w-8 h-8 rounded-lg bg-gradient-to-br from-primary-600 to-primary-400 flex items-center justify-center font-bold text-white shadow-lg shadow-primary-500/20 cursor-default shrink-0">X</div>
        <h1 class="text-xl font-semibold tracking-tight whitespace-nowrap hidden sm:block text-neutral-900 dark:text-white">Xun Workflow</h1>
      </div>
      <div class="flex items-center gap-4 shrink-0">
        
        <!-- 基础操作组（带底色区块包裹，提升整体感） -->
        <div class="flex items-center gap-1.5 bg-neutral-100 dark:bg-neutral-800/60 p-1.5 rounded-lg border border-neutral-200/50 dark:border-neutral-700/50 shadow-sm">
          <!-- 预设模版库 -->
          <div class="relative group shrink-0">
            <button @click="togglePresetMenu" class="hover:bg-white dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-300 px-3 py-1.5 rounded-md font-medium text-sm transition-all active:scale-95 flex items-center gap-1.5 whitespace-nowrap" :class="{'bg-white dark:bg-neutral-700 shadow-sm': showPresetMenu}">
              <!-- 文档合集/书籍图标 -->
              <svg class="w-4 h-4 shrink-0 text-indigo-500 dark:text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"></path></svg>
              <span class="hidden lg:inline">预设模板</span>
              <svg class="w-3.5 h-3.5 shrink-0 transition-transform duration-200 opacity-60" :class="{'rotate-180': showPresetMenu}" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path></svg>
            </button>
            <div v-if="!showPresetMenu" class="absolute top-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">打开预设库</div>
            <!-- 下拉菜单部分保持绝对定位 -->
            <div v-if="showPresetMenu" class="absolute right-0 top-[calc(100%+0.75rem)] w-72 bg-white dark:bg-neutral-900 border border-neutral-300 dark:border-neutral-700 rounded-lg shadow-2xl z-50 overflow-hidden animate-in fade-in slide-in-from-top-2 duration-200">
              <div v-if="presets.length === 0" class="p-4 text-center text-sm text-neutral-500">加载中...</div>
              <div v-for="preset in presets" :key="preset.name" @click="!isLoading && editingPresetName !== preset.name && loadPreset(preset)" class="relative p-4 cursor-pointer border-b border-neutral-200 dark:border-neutral-800/50 last:border-0 transition-colors group/item" :class="[activePresetName === preset.name ? 'bg-indigo-50 dark:bg-indigo-500/10 hover:bg-indigo-100 dark:hover:bg-indigo-500/15' : 'hover:bg-neutral-50 dark:hover:bg-neutral-800/80', isLoading ? 'opacity-50 pointer-events-none' : '']">>
                <!-- 编辑模式：输入框 -->
                <div v-if="editingPresetName === preset.name" class="flex items-center gap-1.5 mb-1 pr-0" @click.stop>
                  <input v-model="editingPresetNewName" @keyup.enter="confirmRenamePreset($event)" @keyup.escape="cancelRenamePreset($event)" class="flex-1 min-w-0 bg-white dark:bg-neutral-800 border border-primary-400 dark:border-primary-600 rounded px-2 py-1 text-sm font-medium text-indigo-600 dark:text-indigo-300 focus:outline-none focus:ring-1 focus:ring-primary-500" autofocus>
                  <button @click.stop="confirmRenamePreset($event)" class="text-emerald-500 hover:text-emerald-400 p-1" title="确认"><svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg></button>
                  <button @click.stop="cancelRenamePreset($event)" class="text-neutral-400 hover:text-neutral-300 p-1" title="取消"><svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12"></path></svg></button>
                </div>
                <!-- 普通模式：显示名称 -->
                <div v-else class="font-medium text-indigo-600 dark:text-indigo-300 text-sm mb-1 pr-14">{{ preset.name }}</div>
                <div class="text-[11px] text-neutral-500 dark:text-neutral-400 leading-relaxed">{{ preset.description }}</div>
                <div v-if="editingPresetName !== preset.name" class="absolute right-3 top-4 opacity-0 group-hover/item:opacity-100 transition-opacity flex items-center gap-1">
                  <div class="relative group/editbtn">
                    <button @click.stop="startRenamePreset(preset.name, $event)" class="text-neutral-400 hover:text-primary-500 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 shadow-sm rounded-md p-1.5" title="重命名预设"><svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"></path></svg></button>
                    <div class="absolute right-[calc(100%+0.5rem)] px-2 py-1 bg-neutral-800 text-white text-[10px] rounded shadow-lg opacity-0 group-hover/editbtn:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">重命名</div>
                  </div>
                  <div class="relative group/delbtn">
                    <button @click.stop="deletePreset(preset.name, $event)" class="text-neutral-400 hover:text-red-500 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 shadow-sm rounded-md p-1.5" title="删除预设"><svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"></path></svg></button>
                    <div class="absolute right-[calc(100%+0.5rem)] px-2 py-1 bg-red-600 text-white text-[10px] rounded shadow-lg opacity-0 group-hover/delbtn:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">删除预设</div>
                  </div>
                </div>
              </div>
            </div>
          </div>
          
          <div class="w-px h-4 bg-neutral-200 dark:bg-neutral-700/50 mx-0.5"></div>

          <!-- 新建空白 -->
          <div class="relative group shrink-0">
            <button @click="clearCanvas" class="hover:bg-white dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-300 px-3 py-1.5 rounded-md text-sm transition-all active:scale-95 flex items-center gap-1.5 whitespace-nowrap">
              <svg class="w-4 h-4 shrink-0 text-emerald-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4"></path></svg>
              <span class="hidden lg:inline">新建</span>
            </button>
            <div class="absolute top-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">创建空白模板</div>
          </div>

          <div class="w-px h-4 bg-neutral-200 dark:bg-neutral-700/50 mx-0.5"></div>

          <!-- 收藏为预设 -->
          <div class="relative group shrink-0">
            <button @click="openPresetModal" class="hover:bg-white dark:hover:bg-neutral-700 text-amber-600 dark:text-amber-400 px-3 py-1.5 rounded-md font-medium text-sm transition-all active:scale-95 flex items-center gap-1.5 whitespace-nowrap">
              <!-- 星星收藏图标 -->
              <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z"></path></svg>
              <span class="hidden lg:inline">存为预设</span>
            </button>
            <div class="absolute top-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">将当前配置存为预设</div>
          </div>

          <!-- 更新当前预设（仅当已加载预设时显示） -->
          <transition
            enter-active-class="transition duration-200 ease-out"
            enter-from-class="opacity-0 scale-95"
            enter-to-class="opacity-100 scale-100"
            leave-active-class="transition duration-150 ease-in"
            leave-from-class="opacity-100 scale-100"
            leave-to-class="opacity-0 scale-95"
          >
          <div v-if="activePresetName" class="relative group shrink-0">
            <button @click="updateActivePreset" :disabled="isLoading" class="hover:bg-white dark:hover:bg-neutral-700 text-cyan-600 dark:text-cyan-400 px-3 py-1.5 rounded-md font-medium text-sm transition-all active:scale-95 flex items-center gap-1.5 whitespace-nowrap disabled:opacity-50 disabled:pointer-events-none">
              <svg :class="isLoading ? 'animate-spin' : ''" class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path></svg>
              <span class="hidden lg:inline">{{ isLoading ? '处理中...' : '更新预设' }}</span>
            </button>
            <div class="absolute top-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">覆盖更新「{{ activePresetName }}」</div>
          </div>
          </transition>
        </div>

        <!-- 外设操作组 -->
        <div class="flex items-center gap-2">
          <!-- 亮暗色切换 -->
          <div class="relative group shrink-0">
            <button @click="toggleTheme" class="bg-white dark:bg-neutral-800 hover:bg-neutral-100 dark:hover:bg-neutral-700 text-neutral-500 dark:text-neutral-400 p-2.5 rounded-lg border border-neutral-200 dark:border-neutral-700 shadow-sm transition-colors">
              <svg v-if="!isDarkMode" class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 3v1m0 16v1m9-9h-1M4 12H3m15.364 6.364l-.707-.707M6.343 6.343l-.707-.707m12.728 0l-.707.707M6.343 17.657l-.707.707M16 12a4 4 0 11-8 0 4 4 0 018 0z"></path></svg>
              <svg v-else class="w-4 h-4 shrink-0 text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M20.354 15.354A9 9 0 018.646 3.646 9.003 9.003 0 0012 21a9.003 9.003 0 008.354-5.646z"></path></svg>
            </button>
            <div class="absolute top-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">{{ isDarkMode ? '切换为亮色模式' : '切换为暗色模式' }}</div>
          </div>

          <!-- 设置 -->
          <div class="relative group shrink-0">
            <button @click="toggleSettingsMenu" class="bg-white dark:bg-neutral-800 hover:bg-neutral-100 dark:hover:bg-neutral-700 text-neutral-500 dark:text-neutral-400 p-2.5 rounded-lg border border-neutral-200 dark:border-neutral-700 shadow-sm transition-colors" :class="{'ring-2 ring-primary-500/50': showSettingsMenu}">
              <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"></path><circle cx="12" cy="12" r="3"></circle></svg>
            </button>
            <div v-if="!showSettingsMenu" class="absolute top-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">设置</div>
            <div v-if="showSettingsMenu" class="absolute right-0 top-[calc(100%+0.5rem)] w-80 bg-white dark:bg-neutral-900 border border-neutral-300 dark:border-neutral-700 rounded-lg shadow-2xl z-50 overflow-hidden animate-in fade-in slide-in-from-top-2 duration-200">
              <div class="p-4">
                <div class="text-sm font-semibold text-neutral-800 dark:text-neutral-200 mb-3">数据存储目录</div>
                <div class="bg-neutral-50 dark:bg-neutral-950 rounded-lg p-3 border border-neutral-200 dark:border-neutral-800 mb-3">
                  <div class="text-[11px] text-neutral-400 dark:text-neutral-500 mb-1">当前路径</div>
                  <div class="text-xs text-neutral-700 dark:text-neutral-300 font-mono break-all leading-relaxed">{{ currentDataDir || '加载中...' }}</div>
                </div>
                <div class="flex gap-2">
                  <button @click="chooseDataDir" class="flex-1 bg-primary-600 hover:bg-primary-500 text-white px-3 py-2 rounded-md text-xs font-medium transition-colors">选择目录</button>
                  <button @click="resetDataDir" class="px-3 py-2 rounded-md text-xs font-medium text-neutral-600 dark:text-neutral-400 bg-neutral-100 dark:bg-neutral-800 hover:bg-neutral-200 dark:hover:bg-neutral-700 border border-neutral-200 dark:border-neutral-700 transition-colors">恢复默认</button>
                </div>
                <p class="text-[10px] text-neutral-400 dark:text-neutral-600 mt-2 leading-relaxed">更改后需重启应用完全生效。配置、预设、历史快照均存于此目录。</p>
              </div>
            </div>
          </div>

        <!-- 快速上手提示词 -->
        <div class="relative group shrink-0">
          <button @click="togglePromptPanel" class="bg-white dark:bg-neutral-800 hover:bg-neutral-100 dark:hover:bg-neutral-700 text-neutral-500 dark:text-neutral-400 p-2.5 rounded-lg border border-neutral-200 dark:border-neutral-700 shadow-sm transition-colors" :class="{'ring-2 ring-primary-500/50': showPromptPanel}">
            <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"></path></svg>
          </button>
          <div v-if="!showPromptPanel" class="absolute top-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">快速上手</div>
          <div v-if="showPromptPanel" class="absolute right-0 top-[calc(100%+0.5rem)] w-[480px] bg-white dark:bg-neutral-900 border border-neutral-300 dark:border-neutral-700 rounded-lg shadow-2xl z-50 overflow-hidden animate-in fade-in slide-in-from-top-2 duration-200">
            <div class="p-4 border-b border-neutral-200 dark:border-neutral-800 flex justify-between items-center">
              <div class="text-sm font-semibold text-neutral-800 dark:text-neutral-200">📋 MCP 接入指南</div>
              <button @click="copyPrompt" class="px-3 py-1.5 rounded-md text-xs font-medium transition-all" :class="promptCopied ? 'bg-emerald-100 dark:bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-200 dark:border-emerald-500/20' : 'bg-primary-600 hover:bg-primary-500 text-white'">
                {{ promptCopied ? '✓ 已复制' : '复制全部' }}
              </button>
            </div>
            <div class="p-4 max-h-80 overflow-y-auto custom-scrollbar">
              <pre class="text-xs text-neutral-700 dark:text-neutral-300 leading-relaxed whitespace-pre-wrap font-mono">{{ mcpUsagePrompt }}</pre>
            </div>
          </div>
        </div>

        <div class="relative group shrink-0">
          <button @click="toggleExportMenu" class="bg-white dark:bg-neutral-800 hover:bg-neutral-100 dark:hover:bg-neutral-700 text-neutral-700 dark:text-neutral-300 px-3 lg:px-4 py-2 rounded-md font-medium text-sm transition-all active:scale-95 shadow-sm border border-neutral-200 dark:border-neutral-700 flex items-center gap-1.5 lg:gap-2 whitespace-nowrap">
            <svg class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4"></path></svg>
            <span class="hidden lg:inline">导入/导出</span>
            <svg class="w-4 h-4 shrink-0 transition-transform duration-200" :class="{'rotate-180': showExportMenu}" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7"></path></svg>
          </button>

          <div v-if="!showExportMenu" class="absolute top-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">
            导入或导出配置文件
          </div>

          <div v-if="showExportMenu" class="absolute right-0 top-[calc(100%+0.5rem)] w-64 bg-white dark:bg-neutral-900 border border-neutral-300 dark:border-neutral-700 rounded-lg shadow-2xl z-50 overflow-hidden animate-in fade-in slide-in-from-top-2 duration-200">
            <div class="p-2 border-b border-neutral-200 dark:border-neutral-800">
              <div class="px-3 py-1.5 text-[10px] font-semibold text-neutral-400 dark:text-neutral-500 uppercase tracking-wider">导出配置</div>
              <button @click="exportConfig('plain-yaml')" class="w-full text-left px-3 py-2.5 hover:bg-neutral-100 dark:hover:bg-neutral-800 rounded-md text-sm transition-colors flex items-center gap-2.5">
                <svg class="w-4 h-4 shrink-0 text-emerald-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"></path></svg>
                <div>
                  <div class="font-medium text-neutral-800 dark:text-neutral-200">白话 YAML</div>
                  <div class="text-[11px] text-neutral-500 dark:text-neutral-500">小白看得懂的中文字段</div>
                </div>
              </button>
              <button @click="exportConfig('yaml')" class="w-full text-left px-3 py-2.5 hover:bg-neutral-100 dark:hover:bg-neutral-800 rounded-md text-sm transition-colors flex items-center gap-2.5">
                <svg class="w-4 h-4 shrink-0 text-blue-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4"></path></svg>
                <div>
                  <div class="font-medium text-neutral-800 dark:text-neutral-200">标准 YAML</div>
                  <div class="text-[11px] text-neutral-500 dark:text-neutral-500">开发者标准格式</div>
                </div>
              </button>
              <button @click="exportConfig('json')" class="w-full text-left px-3 py-2.5 hover:bg-neutral-100 dark:hover:bg-neutral-800 rounded-md text-sm transition-colors flex items-center gap-2.5">
                <svg class="w-4 h-4 shrink-0 text-amber-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z"></path></svg>
                <div>
                  <div class="font-medium text-neutral-800 dark:text-neutral-200">标准 JSON</div>
                  <div class="text-[11px] text-neutral-500 dark:text-neutral-500">通用数据交换格式</div>
                </div>
              </button>
            </div>
            <div class="p-2">
              <div class="px-3 py-1.5 text-[10px] font-semibold text-neutral-400 dark:text-neutral-500 uppercase tracking-wider">导入配置</div>
              <button @click="triggerUpload" class="w-full text-left px-3 py-2.5 hover:bg-neutral-100 dark:hover:bg-neutral-800 rounded-md text-sm transition-colors flex items-center gap-2.5">
                <svg class="w-4 h-4 shrink-0 text-violet-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12"></path></svg>
                <div>
                  <div class="font-medium text-neutral-800 dark:text-neutral-200">从文件导入</div>
                  <div class="text-[11px] text-neutral-500 dark:text-neutral-500">支持 .json / .yaml / .yml</div>
                </div>
              </button>
            </div>
          </div>
        </div>

        <div class="relative group shrink-0">
          <button @click="saveConfig" :disabled="isSaving" class="bg-primary-600 hover:bg-primary-500 disabled:opacity-50 text-white px-4 lg:px-5 py-2 rounded-md font-medium text-sm transition-all active:scale-95 focus:ring-2 focus:ring-primary-500/50 shadow-lg shadow-primary-600/20 flex items-center gap-1.5 shrink-0 whitespace-nowrap">
            <svg v-if="isSaving" class="w-4 h-4 shrink-0 animate-spin" fill="none" viewBox="0 0 24 24" stroke="currentColor"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"></path></svg>
            <svg v-else class="w-4 h-4 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"></path></svg>
            <span class="hidden md:inline">{{ isSaving ? '保存中...' : '应用' }}</span>
          </button>
          <div class="absolute top-[calc(100%+0.5rem)] right-0 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">
            应用当前配置到工作流
          </div>
        </div>
      </div>
      </div>
    </header>

    <div class="flex flex-1 overflow-hidden">
      <!-- 侧边栏: 画布树状图 -->
      <aside class="w-80 shrink-0 border-r border-neutral-200 dark:border-neutral-800 bg-neutral-50 dark:bg-neutral-900/30 flex flex-col transition-colors duration-300">
        <div class="p-4 border-b border-neutral-200 dark:border-neutral-800 flex justify-between items-center shrink-0">
          <h2 class="text-sm font-medium text-neutral-500 dark:text-neutral-400 uppercase tracking-wider">执行流节点 (拖拽排序)</h2>
          <button @click="addNode" class="w-6 h-6 rounded bg-white dark:bg-neutral-800 hover:bg-neutral-100 dark:hover:bg-neutral-700 text-neutral-600 dark:text-neutral-300 flex items-center justify-center transition-colors shadow-sm focus:outline-none border border-neutral-200 dark:border-transparent">+</button>
        </div>
        
        <div class="flex-1 overflow-y-auto custom-scrollbar p-4 space-y-3 relative">
          <!-- 连线背景 -->
          <div class="absolute left-8 top-8 bottom-8 w-px bg-neutral-200 dark:bg-neutral-800 z-0"></div>
          
          <div v-for="(node, index) in config.nodes" :key="nodeKey(node)" 
               draggable="true"
               @dragstart="onDragStart($event, index)"
               @dragenter.prevent="onDragEnter(index)"
               @dragover.prevent
               @drop.prevent="onDrop(index)"
               @dragend="onDragEnd"
               @click="selectNode(index)"
               class="relative z-10 flex items-center gap-3 cursor-pointer group transition-all duration-300 transform"
               :class="{
                  'opacity-40 scale-95': draggedIndex === index,
                  'translate-y-2': dragOverIndex === index && draggedIndex !== null && draggedIndex > index,
                  '-translate-y-2': dragOverIndex === index && draggedIndex !== null && draggedIndex < index
               }">
            
            <div class="w-8 h-8 rounded-full border-2 bg-white dark:bg-neutral-900 flex items-center justify-center text-xs font-mono transition-colors shadow-sm"
                 :class="selectedNodeIndex === index ? 'border-primary-500 text-primary-600 dark:text-primary-400' : 'border-neutral-300 dark:border-neutral-700 text-neutral-400 dark:text-neutral-500 group-hover:border-neutral-400 dark:group-hover:border-neutral-500'">
              {{ index + 1 }}
            </div>
            
            <div class="flex-1 py-3 px-4 rounded-xl border transition-all shadow-sm relative overflow-hidden"
                 :class="selectedNodeIndex === index ? 'bg-primary-50 dark:bg-primary-500/10 border-primary-200 dark:border-primary-500/50 shadow-primary-500/10' : 'bg-white dark:bg-neutral-800/50 border-neutral-200 dark:border-neutral-700/50 group-hover:bg-neutral-50 dark:group-hover:bg-neutral-800'">
              <!-- 拖拽提示柄的微弱高亮 -->
              <div class="absolute left-0 top-0 bottom-0 w-1 bg-gradient-to-b from-transparent via-neutral-200 dark:via-neutral-600 to-transparent opacity-0 group-hover:opacity-100 transition-opacity"></div>
              
              <div class="flex justify-between items-start">
                <div>
                  <h3 class="font-medium text-neutral-800 dark:text-neutral-200" :class="{'text-primary-600 dark:text-primary-300': selectedNodeIndex === index}">{{ node.name || '未命名节点' }}</h3>
                </div>
                <span v-if="node.required" class="px-1.5 py-0.5 rounded text-[10px] font-medium bg-red-100 dark:bg-red-500/10 text-red-600 dark:text-red-400 border border-red-200 dark:border-red-500/20">必做</span>
              </div>
            </div>
          </div>
          
          <div v-if="config.nodes.length === 0" class="text-center p-8 text-neutral-400 dark:text-neutral-600 text-sm">
            暂无节点，点击右上角添加。
          </div>
        </div>
      </aside>

      <!-- 主编辑区 -->
      <main ref="mainPanelRef" class="flex-1 overflow-y-auto custom-scrollbar bg-neutral-50/50 dark:bg-neutral-950 p-8 relative transition-colors duration-300">
        <div v-if="selectedNodeIndex !== null && config.nodes[selectedNodeIndex]" :key="selectedNodeIndex" class="max-w-4xl mx-auto space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
          
          <!-- Node Header -->
          <div class="flex justify-between items-start">
            <div>
              <h2 class="text-2xl font-semibold text-neutral-900 dark:text-white mb-2">节点配置</h2>
              <p class="text-sm text-neutral-500 dark:text-neutral-400">设置当前代理需执行的任务环节及其跳转条件。</p>
            </div>
            <div class="relative group shrink-0">
              <button @click="removeNode(selectedNodeIndex)" class="text-red-500 dark:text-red-400 hover:text-red-600 dark:hover:text-red-300 hover:bg-red-50 dark:hover:bg-red-400/10 px-3 py-1.5 rounded-md text-sm transition-colors border border-transparent hover:border-red-200 dark:hover:border-red-400/20 shrink-0 whitespace-nowrap">
                删除节点
              </button>
              <div class="absolute bottom-[calc(100%+0.5rem)] left-1/2 -translate-x-1/2 px-2.5 py-1.5 bg-neutral-800 text-neutral-200 text-xs rounded-md shadow-lg border border-neutral-700 opacity-0 group-hover:opacity-100 transition-opacity whitespace-nowrap pointer-events-none z-[60]">
                删除当前节点
              </div>
            </div>
          </div>
          
          <!-- Form -->
          <div class="space-y-6 bg-white dark:bg-neutral-900/50 p-6 rounded-2xl border border-neutral-200 dark:border-neutral-800/80 shadow-lg dark:shadow-xl backdrop-blur-sm transition-colors duration-300">
            <div class="grid grid-cols-1 gap-6">
              <div class="space-y-2">
                <label class="text-xs font-semibold text-neutral-500 dark:text-neutral-400 uppercase tracking-wide">节点名称</label>
                <input v-model="config.nodes[selectedNodeIndex].name" @input="onNodeNameInput(selectedNodeIndex)" type="text" class="w-full bg-neutral-50 dark:bg-neutral-950 border border-neutral-200 dark:border-neutral-800 rounded-lg px-4 py-2.5 text-sm text-neutral-900 dark:text-neutral-100 focus:outline-none focus:ring-2 focus:ring-primary-500/50 transition-shadow">
                <p class="text-[10px] text-neutral-400 dark:text-neutral-600 font-mono">ID: {{ config.nodes[selectedNodeIndex].id }}</p>
              </div>
            </div>

            <div class="space-y-2">
              <label class="text-xs font-semibold text-neutral-500 dark:text-neutral-400 uppercase tracking-wide flex justify-between">
                <span>工具执行动作 (ACTION)</span>
              </label>
              <textarea v-model="config.nodes[selectedNodeIndex].action" rows="3" placeholder="该节点需要 AI 调用什么工具或做什么操作？描述得越清楚越好。" class="w-full bg-neutral-50 dark:bg-neutral-950 border border-neutral-200 dark:border-neutral-800 rounded-lg px-4 py-3 text-sm text-neutral-900 dark:text-neutral-100 focus:outline-none focus:ring-2 focus:ring-primary-500/50 transition-shadow resize-none"></textarea>
            </div>

            <div class="flex items-center gap-3 pt-2">
              <button 
                @click="config.nodes[selectedNodeIndex].required = !config.nodes[selectedNodeIndex].required"
                class="w-12 h-6 rounded-full relative transition-colors duration-300 focus:outline-none focus:ring-2 focus:ring-primary-500/50 focus:ring-offset-2 focus:ring-offset-white dark:focus:ring-offset-neutral-900 shrink-0"
                :class="config.nodes[selectedNodeIndex].required ? 'bg-primary-500' : 'bg-neutral-300 dark:bg-neutral-700'"
              >
                <div class="absolute top-1 left-1 bg-white w-4 h-4 rounded-full transition-transform duration-300 shadow-sm"
                     :class="config.nodes[selectedNodeIndex].required ? 'translate-x-6' : 'translate-x-0'"></div>
              </button>
              <div class="space-y-1">
                <span class="text-sm font-medium text-neutral-800 dark:text-neutral-200">强制必做 (不可被自主跳过)</span>
                <p class="text-xs text-neutral-500 dark:text-neutral-500 block leading-relaxed">如果开启此项，即便 AI 自作主张跳过该步骤，也会在自检时被底层系统强行拦截并要求返工。</p>
              </div>
            </div>

            <!-- 循环回退目标 -->
            <div class="space-y-2 pt-2">
              <label class="text-xs font-semibold text-neutral-500 dark:text-neutral-400 uppercase tracking-wide">循环回退目标 (LOOP BACK TO)</label>
              <select v-model="config.nodes[selectedNodeIndex].loop_back_to"
                class="w-full bg-neutral-50 dark:bg-neutral-950 border border-neutral-200 dark:border-neutral-800 rounded-lg px-4 py-2.5 text-sm text-neutral-900 dark:text-neutral-100 focus:outline-none focus:ring-2 focus:ring-primary-500/50 transition-shadow">
                <option :value="null">不循环（默认）</option>
                <option v-for="node in config.nodes.filter((_n, i) => i !== selectedNodeIndex)" :key="node.id" :value="node.id">
                  {{ node.name }}
                </option>
              </select>
              <p class="text-[10px] text-neutral-400 dark:text-neutral-600 leading-relaxed">
                设置后，当此节点触发循环时（如用户反馈"继续"），AI 将回退到目标节点重新执行。不填则不触发循环。
              </p>
            </div>

            <div class="pt-4 border-t border-neutral-200 dark:border-neutral-800/80 relative">
              <!-- required=true 时覆盖置灰遮罩 -->
              <div v-if="config.nodes[selectedNodeIndex].required" class="absolute inset-0 bg-white/60 dark:bg-neutral-900/70 z-10 rounded-lg flex items-center justify-center backdrop-blur-[1px]">
                <div class="flex items-center gap-2 px-4 py-2 rounded-full bg-amber-50 dark:bg-amber-500/10 border border-amber-200 dark:border-amber-500/20 text-amber-700 dark:text-amber-400 text-xs font-medium shadow-sm">
                  <svg class="w-3.5 h-3.5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"></path></svg>
                  已启用强制必做，跳过条件将被忽略
                </div>
              </div>
              <div class="flex justify-between items-center mb-4">
                <label class="text-xs font-semibold text-neutral-500 dark:text-neutral-400 uppercase tracking-wide">智能跳过条件 (SKIP WHEN) - 满足条件时合法跳过此节点</label>
                <button @click="addSkipCondition" class="text-xs text-primary-600 dark:text-primary-400 hover:text-primary-700 dark:hover:text-primary-300 hover:bg-primary-50 dark:hover:bg-primary-400/10 px-2 py-1 rounded transition-colors">+ 添加条件</button>
              </div>
              
              <div class="space-y-2">
                <div v-for="(_cond, idx) in config.nodes[selectedNodeIndex].skip_when" :key="idx" class="flex gap-2">
                  <input v-model="config.nodes[selectedNodeIndex].skip_when[idx]" type="text" placeholder="大白话例如：'如果不是修改架构时跳过'。或内部代号：'complexity:simple'" class="flex-1 bg-neutral-50 dark:bg-neutral-950 border border-neutral-200 dark:border-neutral-800 rounded-lg px-4 py-2 text-sm text-neutral-900 dark:text-neutral-100 focus:outline-none focus:ring-2 focus:ring-primary-500/50 transition-shadow">
                  <button @click="removeSkipCondition(idx)" class="w-10 h-10 flex items-center justify-center rounded-lg border border-neutral-200 dark:border-neutral-800 hover:bg-red-50 dark:hover:bg-red-500/10 hover:border-red-300 dark:hover:border-red-500/30 hover:text-red-500 dark:hover:text-red-400 text-neutral-400 dark:text-neutral-500 transition-colors bg-white dark:bg-transparent">×</button>
                </div>
                <div v-if="config.nodes[selectedNodeIndex].skip_when.length === 0" class="text-xs text-neutral-500 dark:text-neutral-600 bg-neutral-50 dark:bg-neutral-950/50 p-4 rounded-lg border border-neutral-200 dark:border-neutral-800/50 border-dashed text-center">
                  该节点无任何跳过条件，AI 将必须执行。
                </div>
              </div>

              <!-- 常用条件速选 -->
              <div class="mt-4 pt-3 border-t border-neutral-100 dark:border-neutral-800/50">
                <div class="text-[10px] font-semibold text-neutral-400 dark:text-neutral-500 uppercase tracking-wider mb-2">常用条件速选（点击一键添加）</div>
                <div class="flex flex-wrap gap-1.5">
                  <button v-for="preset in [
                    { value: 'complexity:simple', label: '☁️ 简单任务时跳过' },
                    { value: 'complexity:medium', label: '⚡ 中等任务时跳过' },
                    { value: 'complexity:complex', label: '🔥 复杂任务时跳过' },
                    { value: '用户没有明确的任务意图时跳过', label: '💬 无明确任务意图' },
                    { value: '用户只是打招呼问候时跳过', label: '👋 只是打招呼' },
                    { value: '仅做配置类修改时跳过', label: '⚙️ 仅配置变更' }
                  ]" :key="preset.value"
                    @click="!config.nodes[selectedNodeIndex].skip_when.includes(preset.value) && config.nodes[selectedNodeIndex].skip_when.push(preset.value)"
                    class="inline-flex items-center gap-1 px-2.5 py-1.5 rounded-lg text-[11px] font-medium border transition-all"
                    :class="config.nodes[selectedNodeIndex].skip_when.includes(preset.value)
                      ? 'bg-neutral-100 dark:bg-neutral-800 text-neutral-400 dark:text-neutral-600 border-neutral-200 dark:border-neutral-700 cursor-default opacity-50'
                      : 'bg-white dark:bg-neutral-900 text-neutral-600 dark:text-neutral-300 border-neutral-200 dark:border-neutral-700 hover:border-primary-300 dark:hover:border-primary-500/40 hover:text-primary-600 dark:hover:text-primary-400 cursor-pointer active:scale-95'">
                    {{ preset.label }}
                  </button>
                </div>
                <p class="text-[10px] text-neutral-400 dark:text-neutral-600 mt-2 leading-relaxed">
                  💡 你也可以用「+ 添加条件」自由输入任意白话描述，AI 会自动理解你的意图。
                </p>
              </div>
            </div>
          </div>
        </div>
        
        <div v-else class="h-full flex flex-col items-center justify-center text-neutral-400 dark:text-neutral-500">
           <div class="w-16 h-16 rounded-2xl bg-white dark:bg-neutral-900 border border-neutral-200 dark:border-neutral-800 flex items-center justify-center mb-4 shadow-sm dark:shadow-inner">
             <svg class="w-8 h-8 opacity-50 text-neutral-300 dark:text-neutral-600" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M14 10l-2 1m0 0l-2-1m2 1v2.5M20 7l-2 1m2-1l-2-1m2 1v2.5M14 4l-2-1-2 1M4 7l2-1M4 7l2 1M4 7v2.5M12 21l-2-1m2 1l2-1m-2 1v-2.5M6 18l-2-1v-2.5M18 18l2-1v-2.5" /></svg>
           </div>
           <p>请在左侧选择一个执行节点或新建节点以进行配置。</p>
        </div>
      </main>
    </div>

    <!-- 预设独立保存面板 (Modal) -->
    <div v-if="showPresetModal" class="fixed inset-0 bg-black/60 backdrop-blur-sm z-[100] flex items-center justify-center transition-all">
      <div class="bg-white dark:bg-neutral-900 border border-neutral-300 dark:border-neutral-700 rounded-2xl shadow-2xl w-full max-w-md overflow-hidden transform transition-all translate-y-0 opacity-100">
        <div class="p-6">
          <h3 class="text-xl font-semibold text-neutral-900 dark:text-white mb-2">✨ 存入预设模版库</h3>
          <p class="text-sm text-neutral-600 dark:text-neutral-400 mb-6">将当前画布的拼装成果与执行流保存为一套长效的独立预设。</p>
          
          <div class="space-y-4">
            <div class="space-y-2">
              <label class="text-xs font-semibold text-neutral-600 dark:text-neutral-400 uppercase tracking-wide">预设名称 (必填)</label>
              <input v-model="newPresetName" type="text" placeholder="例如：超长文本摘要版" class="w-full bg-neutral-50 dark:bg-neutral-950 border border-neutral-300 dark:border-neutral-700 rounded-lg px-4 py-3 text-sm focus:outline-none focus:ring-2 focus:ring-primary-500/50 transition-shadow">
            </div>
            <div class="space-y-2">
              <label class="text-xs font-semibold text-neutral-600 dark:text-neutral-400 uppercase tracking-wide">预设特色描述</label>
              <textarea v-model="newPresetDesc" rows="2" placeholder="一句话描述它的特色" class="w-full bg-neutral-50 dark:bg-neutral-950 border border-neutral-300 dark:border-neutral-700 rounded-lg px-4 py-3 text-sm focus:outline-none focus:ring-2 focus:ring-primary-500/50 transition-shadow resize-none"></textarea>
            </div>
          </div>
        </div>
        <div class="p-4 bg-neutral-50 dark:bg-neutral-950 border-t border-neutral-200 dark:border-neutral-800 flex justify-end gap-3 rounded-b-2xl">
          <button @click="showPresetModal = false" class="px-5 py-2 text-sm font-medium text-neutral-600 dark:text-neutral-400 hover:text-white transition-colors bg-white dark:bg-neutral-900 hover:bg-neutral-100 dark:bg-neutral-800 rounded-md border border-neutral-200 dark:border-neutral-800">取消</button>
          <button @click="confirmSavePreset" :disabled="!newPresetName.trim()" class="bg-primary-600 hover:bg-primary-500 disabled:opacity-50 text-white px-6 py-2 rounded-md font-medium text-sm transition-all shadow-lg focus:outline-none focus:ring-2 focus:ring-primary-500/50">确认保存</button>
        </div>
      </div>
    </div>
    <!-- 通用红框动作提醒悬浮窗 (Confirm Modal) -->
    <div v-if="confirmModal.show" class="fixed inset-0 bg-black/60 backdrop-blur-sm z-[110] flex items-center justify-center transition-all">
      <div class="bg-white dark:bg-neutral-900 border border-neutral-300 dark:border-neutral-700 rounded-2xl shadow-2xl w-full max-w-sm overflow-hidden transform transition-all translate-y-0 opacity-100">
        <div class="p-6">
          <h3 class="text-xl font-semibold text-neutral-900 dark:text-white mb-2">{{ confirmModal.title }}</h3>
          <p class="text-sm text-neutral-600 dark:text-neutral-400 leading-relaxed">{{ confirmModal.message }}</p>
        </div>
        <div class="p-4 bg-neutral-50 dark:bg-neutral-950 border-t border-neutral-200 dark:border-neutral-800 flex justify-end gap-3 rounded-b-2xl">
          <button @click="confirmModal.show = false" class="px-5 py-2 text-sm font-medium text-neutral-600 dark:text-neutral-400 hover:text-white transition-colors bg-white dark:bg-neutral-900 hover:bg-neutral-100 dark:bg-neutral-800 rounded-md border border-neutral-200 dark:border-neutral-800">取消</button>
          <button @click="executeConfirm" class="bg-red-600 hover:bg-red-500 text-white px-6 py-2 rounded-md font-medium text-sm transition-all shadow-lg focus:outline-none focus:ring-2 focus:ring-red-500/50">确定</button>
        </div>
      </div>
    </div>
    </div>
  </main>
</template>

<style>
/* Remove the heavy default styles from original vue template here */
body, html {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden; /* 彻底切断浏览器级别的全局滚动，由内部框架接管 */
}
#app {
  width: 100%;
  height: 100%;
}

/* 核心强调色变量体系（配合 Tailwind Primary 通道） */
:root, .dark {
  --color-primary-300: 165 180 252;
  --color-primary-400: 129 140 248;
  --color-primary-500: 99 102 241;
  --color-primary-600: 79 70 229;
}

/* 深色质感专属滚动条配置 */
.custom-scrollbar::-webkit-scrollbar {
  width: 6px;
  height: 6px;
}
.custom-scrollbar::-webkit-scrollbar-track {
  background: transparent;
}
.custom-scrollbar::-webkit-scrollbar-thumb {
  background-color: #3f3f46;
  border-radius: 10px;
}
.custom-scrollbar::-webkit-scrollbar-thumb:hover {
  background-color: #52525b;
}
</style>