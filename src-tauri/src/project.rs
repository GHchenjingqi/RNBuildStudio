//! 子应用注册表与 app.config.json 的读写。
//!
//! 边界（与基座 README「项目边界」一致）：GUI 只读写**真源文件**
//! （app-registry.json / <子应用>/app.config.json）；version.json / res/ / h5/
//! 这些派生产物一律由基座脚本（apply-app-meta.js / use-app.js）生成，这里不碰。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// 强特征判断：目录里同时存在 `app-registry.json` 与 `scripts/apply-app-meta.js`
/// 即可认定是 RNbase 基座（避免只靠目录名误判）。
fn is_rnbase(dir: &Path) -> bool {
    dir.join("app-registry.json").exists() && dir.join("scripts").join("apply-app-meta.js").exists()
}

fn registry_path(base_dir: &str) -> PathBuf {
    Path::new(base_dir).join("app-registry.json")
}

fn app_dir(base_dir: &str, app_name: &str) -> Result<PathBuf, String> {
    let raw = fs::read_to_string(registry_path(base_dir))
        .map_err(|e| format!("读取 app-registry.json 失败：{e}"))?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("app-registry.json 解析失败：{e}"))?;
    let dir = v["apps"][app_name]["dir"]
        .as_str()
        .ok_or_else(|| format!("注册表中不存在子应用 {app_name}"))?;
    // 注册表里的 dir 是相对基座的路径（如 ../app_h5）
    Ok(Path::new(base_dir).join(dir))
}

/// 逐个检查注册表里的子应用是否还「活着」：目录在不在、app.config.json 在不在。
///
/// 为什么需要：用户手工删掉 / 改名了子应用目录时，注册表里的记录会变成悬空指针，
/// 前端再把 activeApp 指向它就会在读配置这一步报错。这里提前把体检结果交给前端，
/// 让它跳过悬空项并给出可操作的提示，而不是抛一个看不懂的 IO 错误。
fn check_health(base_dir: &str, registry: &Value) -> Value {
    let empty = serde_json::Map::new();
    let apps = registry["apps"].as_object().unwrap_or(&empty);
    let mut map = serde_json::Map::new();
    for (name, entry) in apps {
        let rel = entry["dir"].as_str().unwrap_or("");
        let dir = Path::new(base_dir).join(rel);
        let dir_exists = dir.is_dir();
        let config_exists = dir.join("app.config.json").is_file();
        let mut m = serde_json::Map::new();
        m.insert("dir".into(), Value::String(dir.to_string_lossy().to_string()));
        m.insert("dirExists".into(), Value::Bool(dir_exists));
        m.insert("configExists".into(), Value::Bool(config_exists));
        m.insert("ok".into(), Value::Bool(dir_exists && config_exists));
        map.insert(name.clone(), Value::Object(m));
    }
    Value::Object(map)
}

/// 读取子应用注册表（activeApp + 各子应用目录），并附带 health 体检结果。
///
/// 返回的 `health` 是实时算出来的附加字段，不存在注册表文件里。
#[tauri::command]
pub fn list_apps(base_dir: String) -> Result<Value, String> {
    let raw = fs::read_to_string(registry_path(&base_dir))
        .map_err(|e| format!("读取 app-registry.json 失败：{e}"))?;
    let mut v: Value =
        serde_json::from_str(&raw).map_err(|e| format!("app-registry.json 解析失败：{e}"))?;
    let health = check_health(&base_dir, &v);
    if let Some(obj) = v.as_object_mut() {
        obj.insert("health".into(), health);
    }
    Ok(v)
}

/// 把子应用从注册表注销（用于清理目录已被删除的悬空记录）。
///
/// 只动 app-registry.json 这一份真源文件，不碰任何子应用目录。
/// 若被注销的正是 activeApp，则把 activeApp 清空——留着一个指向不存在应用的
/// 活动指针会让后续所有脚本失败，宁可让前端重新挑一个。
#[tauri::command]
pub fn unregister_app(base_dir: String, app_name: String) -> Result<(), String> {
    let p = registry_path(&base_dir);
    let raw = fs::read_to_string(&p).map_err(|e| format!("读取 app-registry.json 失败：{e}"))?;
    let mut v: Value = serde_json::from_str(&raw).map_err(|e| format!("app-registry.json 解析失败：{e}"))?;
    let removed = v["apps"]
        .as_object_mut()
        .map(|m| m.remove(&app_name).is_some())
        .unwrap_or(false);
    if !removed {
        // 幂等：记录本来就不存在，当作已清理
        return Ok(());
    }
    if v["activeApp"].as_str() == Some(app_name.as_str()) {
        v["activeApp"] = Value::String(String::new());
    }
    let body = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    fs::write(&p, body + "\n").map_err(|e| format!("写入 app-registry.json 失败：{e}"))
}

/// 读取子应用的 app.config.json（版本与品牌唯一真源）。
#[tauri::command]
pub fn read_app_config(base_dir: String, app_name: String) -> Result<Value, String> {
    let dir = app_dir(&base_dir, &app_name)?;
    // 目录被删/改名是最常见的悬空场景，单独给出能直接照做的提示
    if !dir.is_dir() {
        return Err(format!(
            "子应用「{app_name}」的目录已不存在：{}\n请在「新建子应用」页重建，或在本页点「从注册表移除」清理这条记录。",
            dir.display()
        ));
    }
    let p = dir.join("app.config.json");
    let raw = fs::read_to_string(&p)
        .map_err(|e| format!("读取「{app_name}」的 app.config.json 失败（{}）：{e}", p.display()))?;
    serde_json::from_str(&raw).map_err(|e| format!("app.config.json 解析失败：{e}"))
}

/// 写回子应用的 app.config.json（pretty + 换行，与脚本写入格式一致）。
#[tauri::command]
pub fn write_app_config(base_dir: String, app_name: String, config: Value) -> Result<(), String> {
    let dir = app_dir(&base_dir, &app_name)?;
    if !dir.is_dir() {
        return Err(format!(
            "子应用「{app_name}」的目录已不存在：{}，无法写入配置",
            dir.display()
        ));
    }
    let p = dir.join("app.config.json");
    let body = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    fs::write(&p, body + "\n").map_err(|e| format!("写入 app.config.json 失败：{e}"))
}

/// 自动探测 RNbase 基座目录（零配置核心）：
/// 1. exe 自身所在目录就是 RNbase —— 对应「把 exe 丢进 RNbase 根目录即用」的发布形态；
/// 2. 开发态：exe 深埋在 `RNBuildStudio/src-tauri/target/...`，向上遍历祖先，
///    在每个祖先的父目录里找名为 `RNbase` 的兄弟目录并校验；
/// 3. 退回当前工作目录；都找不到返回空串（前端提示用户手动选择）。
#[tauri::command]
pub fn detect_base_dir() -> String {
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            if is_rnbase(parent) {
                return parent.to_string_lossy().to_string();
            }
            // 向上遍历祖先，检查「祖先的父目录」下的兄弟里有没有 RNbase
            let mut cur = Some(parent);
            while let Some(d) = cur {
                if let Some(grand) = d.parent() {
                    if let Ok(entries) = fs::read_dir(grand) {
                        for e in entries.flatten() {
                            if e.file_name().to_string_lossy().to_lowercase() == "rnbase"
                                && is_rnbase(&e.path())
                            {
                                return e.path().to_string_lossy().to_string();
                            }
                        }
                    }
                }
                cur = d.parent();
            }
        }
    }
    // 退回当前工作目录
    if let Ok(cwd) = env::current_dir() {
        if is_rnbase(&cwd) {
            return cwd.to_string_lossy().to_string();
        }
    }
    String::new()
}

/// 校验给定目录能否作为基座目录（设置页实时状态提示用）。
#[tauri::command]
pub fn validate_base_dir(path: String) -> bool {
    is_rnbase(Path::new(&path))
}

/// 扫描基座同级目录里「已存在、但尚未登记」的子应用目录。
///
/// 场景：用户早就有子应用工程（如 gushici_h5 / jingyitest），只是没在 GUI 里登记，
/// 现在只想选它出包、不想再走「新建」建一遍。基座约定子应用建在同级目录，
/// 故扫描 base_dir 的父目录即可。每个含 app.config.json 的同级目录返回一条摘要。
#[tauri::command]
pub fn scan_subapps(base_dir: String) -> Result<Value, String> {
    let base = Path::new(&base_dir);
    let parent = base
        .parent()
        .ok_or_else(|| "基座目录没有父目录，无法扫描同级子应用".to_string())?;

    // 已登记的名称：注册表的键名不一定等于目录名（目录 gushici_h5 可能登记为 gushici，
    // RN_h5 登记为 rn_h5），所以不能按 basename 判断是否已登记，而要按「解析后的目录绝对路径」
    // 匹配，并把命中的键名回传（registeredAs），供前端直接选中。
    let reg: Value = fs::read_to_string(registry_path(&base_dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::json!({ "apps": {} }));
    let mut key_by_dir: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some(apps) = reg["apps"].as_object() {
        for (key, entry) in apps {
            if let Some(rel) = entry["dir"].as_str() {
                let abs = base.join(rel);
                key_by_dir.insert(abs.to_string_lossy().to_lowercase(), key.clone());
            }
        }
    }

    let mut found: Vec<Value> = Vec::new();
    let entries = fs::read_dir(parent).map_err(|e| format!("读取同级目录失败：{e}"))?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let dir = e.path();
        if !dir.is_dir() {
            continue;
        }
        let cfg_path = dir.join("app.config.json");
        if !cfg_path.is_file() {
            continue;
        }
        let cfg: Value = fs::read_to_string(&cfg_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null);
        // dist / built 推断与基座 active-app.js detectDist 对齐：dist-file 优先于 dist
        let has_dist_file = dir.join("dist-file").join("index.html").is_file();
        let has_dist = has_dist_file || dir.join("dist").join("index.html").is_file();
        let dist = if has_dist_file { "dist-file" } else { "dist" };
        let dir_low = dir.to_string_lossy().to_lowercase();
        let registered_as = key_by_dir.get(&dir_low).cloned().unwrap_or_default();
        let mut m = serde_json::Map::new();
        m.insert("name".into(), Value::String(name));
        m.insert("dir".into(), Value::String(dir.to_string_lossy().to_string()));
        m.insert("appName".into(), cfg["appName"].clone());
        m.insert("applicationId".into(), cfg["applicationId"].clone());
        m.insert("dist".into(), Value::String(dist.into()));
        m.insert("built".into(), Value::Bool(has_dist));
        m.insert("registered".into(), Value::Bool(!registered_as.is_empty()));
        m.insert("registeredAs".into(), Value::String(registered_as));
        found.push(Value::Object(m));
    }
    Ok(Value::Array(found))
}

/// 把一个已存在的子应用目录登记进注册表（幂等）。
///
/// 只写 app-registry.json 这一份真源。dir 以相对基座的路径记录（约定子应用建在同级，
/// 即 `../<目录名>`；非同级则退回绝对路径，path::resolve 对绝对路径直接返回，同样可用）。
/// 已登记则跳过，不覆盖已有 dist。
#[tauri::command]
pub fn register_app(base_dir: String, app_name: String, dir: String, dist: String) -> Result<(), String> {
    let p = registry_path(&base_dir);
    let mut v: Value = fs::read_to_string(&p)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::json!({ "activeApp": "", "apps": {} }));
    if v["apps"].is_null() {
        v["apps"] = serde_json::json!({});
    }
    let apps = v["apps"]
        .as_object_mut()
        .ok_or_else(|| "app-registry.json 的 apps 不是对象".to_string())?;
    if apps.contains_key(&app_name) {
        return Ok(()); // 幂等：同名键已存在，不覆盖
    }
    // 按目录路径再兜一层：同一目录可能已用另一个键名登记（如目录 gushici_h5 登记为 gushici），
    // 此时不应再插入重复键（否则注册表里两条记录指向同一目录）。
    let base_abs = Path::new(base_dir.trim_end_matches(['\\', '/']));
    let dir_low = Path::new(&dir).to_string_lossy().to_lowercase();
    let already_by_dir = apps.values().any(|e| {
        e["dir"]
            .as_str()
            .map(|rel| base_abs.join(rel).to_string_lossy().to_lowercase() == dir_low)
            .unwrap_or(false)
    });
    if already_by_dir {
        return Ok(());
    }
    // 同级目录 => ../<basename>；否则用绝对路径
    let base_parent = base_abs.parent();
    let dir_path = Path::new(&dir);
    let rel = if base_parent.is_some() && base_parent == dir_path.parent() {
        format!(
            "../{}",
            dir_path.file_name().unwrap_or_default().to_string_lossy()
        )
    } else {
        dir.replace('\\', "/")
    };
    let mut entry = serde_json::Map::new();
    entry.insert("dir".into(), Value::String(rel));
    entry.insert(
        "dist".into(),
        Value::String(if dist.trim().is_empty() { "dist".into() } else { dist.trim().into() }),
    );
    apps.insert(app_name.clone(), Value::Object(entry));
    let body = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    fs::write(&p, body + "\n").map_err(|e| format!("写入 app-registry.json 失败：{e}"))
}
