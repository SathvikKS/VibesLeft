use tauri::Manager;

mod error;
mod usage;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .invoke_handler(tauri::generate_handler![greet, usage::get_usage_report])
        .setup(|app| {
            #[cfg(debug_assertions)]
            for provider in &["claude", "antigravity", "codex"] {
                let path = std::env::temp_dir()
                    .join(format!("vibes-left-token-cache-{provider}.json"));
                eprintln!("[creds_cache] {} cache path: {}", provider, path.display());
            }
            app.manage(usage::UsageManager::new(app.handle().clone())?);
            Ok(())
        });

    #[cfg(debug_assertions)]
    {
        builder = builder.plugin(tauri_plugin_mcp_bridge::init());
    }

    builder
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
