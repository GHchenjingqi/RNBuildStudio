//! 品牌图（图标 / 闪屏主图）的选取与导入。
//!
//! 边界（与基座 README 一致）：GUI 只负责把用户选中的**主图**拷进
//! `<目标应用>/app-brand/`（icon.png / splash.png，真源）；多密度切图是派生产物，
//! 一律由基座脚本 `scripts/gen-brand.js` 生成，这里不碰 res/。
//!
//! 为什么选图用 rfd 而不是 <input type="file">：WebView 出于安全不暴露真实路径，
//! 而后续 gen-brand / 预览都需要绝对路径，必须走原生文件对话框。

use std::fs;
use std::path::Path;

use serde_json::{json, Value};

/// PNG 魔数：提前拦下选错文件的情况，而不是等 gen-brand 跑到一半报「不是 PNG」。
const PNG_MAGIC: [u8; 8] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

fn is_png(path: &str) -> Result<(), String> {
    let mut head = [0u8; 8];
    let mut f = fs::File::open(path).map_err(|e| format!("无法读取文件 {path}：{e}"))?;
    let n = std::io::Read::read(&mut f, &mut head).map_err(|e| format!("读取文件头失败 {path}：{e}"))?;
    if n < 8 || head != PNG_MAGIC {
        return Err(format!("{path} 不是 PNG 文件（请导出 PNG 格式的主图）"));
    }
    Ok(())
}

/// 弹出原生文件选择框，返回选中的 PNG 绝对路径（取消返回 null）。
///
/// 用 async + spawn_blocking：对话框是模态阻塞调用，不能占死 Tauri 的主线程 /
/// IPC 线程，否则窗口拖动、日志事件都会卡住。
#[tauri::command]
pub async fn pick_image(title: String) -> Option<String> {
    tauri::async_runtime::spawn_blocking(move || {
        rfd::FileDialog::new()
            .set_title(if title.is_empty() { "选择 PNG 主图" } else { &title })
            .add_filter("PNG 图片", &["png"])
            .pick_file()
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .ok()
    .flatten()
}

/// 把选中的图标 / 闪屏主图拷贝为应用的品牌真源：
///   `<target_dir>/app-brand/icon.png`、`<target_dir>/app-brand/splash.png`
///
/// 两个入参相互独立：只传一个就只拷一个；都为空则什么都不做（返回 copied 全 false），
/// 前端据此决定是否还要跑 gen-brand。已存在的同名主图会被覆盖（用户换图是常态）。
#[tauri::command]
pub fn import_brand_images(
    target_dir: String,
    icon_path: Option<String>,
    splash_path: Option<String>,
) -> Result<Value, String> {
    let brand_dir = Path::new(&target_dir).join("app-brand");
    let mut copied_icon = false;
    let mut copied_splash = false;

    if let Some(src) = icon_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        is_png(src)?;
        fs::create_dir_all(&brand_dir)
            .map_err(|e| format!("创建品牌目录失败 {}：{e}", brand_dir.display()))?;
        fs::copy(src, brand_dir.join("icon.png"))
            .map_err(|e| format!("拷贝图标主图失败：{e}"))?;
        copied_icon = true;
    }
    if let Some(src) = splash_path.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        is_png(src)?;
        fs::create_dir_all(&brand_dir)
            .map_err(|e| format!("创建品牌目录失败 {}：{e}", brand_dir.display()))?;
        fs::copy(src, brand_dir.join("splash.png"))
            .map_err(|e| format!("拷贝闪屏主图失败：{e}"))?;
        copied_splash = true;
    }

    Ok(json!({
        "brandDir": brand_dir.to_string_lossy(),
        "icon": copied_icon,
        "splash": copied_splash,
    }))
}
