// 循（Xun）MCP 服务器入口点
// 注意：stdio 传输要求 stdout 只输出 JSON-RPC，所有日志必须走 stderr

use xun_lib::workflow_mcp::run_workflow_server;

#[tokio::main]
async fn main() {
    // panic hook：确保 panic 信息输出到 stderr（不污染 stdout JSON-RPC）
    std::panic::set_hook(Box::new(|info| {
        eprintln!("[workFlow MCP PANIC] {}", info);
    }));

    // 显式指定 stderr，防止任何日志输出污染 stdout JSON-RPC 通道
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();

    log::info!("启动 循(Xun) 工作流 MCP 服务器 (pid={})", std::process::id());

    if let Err(e) = run_workflow_server().await {
        log::error!("MCP 服务器异常退出: {:#}", e);
        eprintln!("MCP 服务器异常退出: {:#}", e);
        std::process::exit(1);
    }
}
