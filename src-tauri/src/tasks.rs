//! 任务编排：spawn 基座脚本（node scripts/xxx.js），stdout/stderr 逐行经事件推给前端。
//!
//! 设计约束（来自 RNbase 打包链路的实际行为）：
//!  - 全局单并发：gradle 与 hvigor 不能同时跑（会 OOM），前端用任务锁保证一次一个；
//!  - hdc 永远返回 0，成功与否只能解析输出，所以 stdout/stderr 都要完整转发到前端；
//!  - 双击 exe 启动时不继承 shell 的 PATH，工具链路径由前端设置页注入 envs。

use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// 运行中的子进程注册表（task_id -> child），供 stop_task 终止。
#[derive(Default)]
pub struct TaskRegistry(pub Mutex<HashMap<String, Child>>);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskLine {
    pub task_id: String,
    pub stream: String,
    pub line: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskExit {
    pub task_id: String,
    pub code: Option<i32>,
    pub success: bool,
}

/// 逐行泵送子进程输出；非 UTF-8 字节用 U+FFFD 替换，绝不因此中断读取。
///
/// 为什么不能直接 `BufReader::new(out).lines().map_while(Result::ok)`：
/// Windows 下 gradle / JDK 会吐 GBK 字节，`lines()` 遇到非法 UTF-8 就返回 Err，
/// `map_while(Result::ok)` 会**直接结束循环** → `ChildStdout` 被 drop → 管道读端
/// 关闭 → 基座侧 gradle.js 再写就是 EPIPE，整个打包被打成「失败」（构建其实还在跑，
/// 甚至已经跑完，只是没人收尾）。读端必须活到子进程退出为止。
///
/// 为什么还要处理 `\r`：Gradle 分发包下载器（gradlew）用**回车覆盖同一行**刷进度
/// （`12%\r 34%\r …`，整段不换行）。旧实现只按 `\n` 转发，于是整个下载期间
/// 累积的字节一直憋在缓冲里、UI 一行都收不到 → 用户以为卡死、没有进度条。
/// 这里把每个 `\r` 当成一帧进度转发出去（相同帧去重，避免刷屏），让前端能显示
/// 「正在下载（xx%）」。正常以 `\n` 结尾的日志行完全不受影响。
fn pump_lines<R: Read>(reader: R, mut emit: impl FnMut(String)) {
    let mut reader = BufReader::new(reader);
    let mut pending: Vec<u8> = Vec::with_capacity(1024);
    let mut last_frame: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = match reader.read(&mut chunk) {
            Ok(0) => break, // EOF：子进程结束
            Ok(n) => n,
            Err(e) => {
                emit(format!("<读取输出失败：{e}>"));
                break;
            }
        };
        for &b in &chunk[..n] {
            match b {
                b'\n' => {
                    // 行结束：去掉行尾可能残留的 \r，发出并清空
                    while pending.last() == Some(&b'\r') {
                        pending.pop();
                    }
                    if !pending.is_empty() {
                        emit(String::from_utf8_lossy(&pending).into_owned());
                        pending.clear();
                    }
                }
                b'\r' => {
                    // 回车覆盖：把当前这一帧作为一条进度行转发（相同帧去重），
                    // 然后清空，等下载器下一段字节从行首继续覆盖、累积成下一帧。
                    if !pending.is_empty() {
                        if pending != last_frame {
                            emit(String::from_utf8_lossy(&pending).into_owned());
                            last_frame = pending.clone();
                        }
                        pending.clear();
                    }
                }
                _ => pending.push(b),
            }
        }
    }
    // EOF：残留的未换行内容（下载进度的最后一条可能没结尾换行）也补发出来。
    if !pending.is_empty() {
        emit(String::from_utf8_lossy(&pending).into_owned());
    }
}

/// 启动一个基座脚本任务。事件流：
///  - `task-line`：TaskLine（stdout/stderr 逐行）
///  - `task-exit`：TaskExit（退出码；被 kill 时 code 为 None）
#[tauri::command]
pub fn run_task(
    app: AppHandle,
    registry: State<'_, TaskRegistry>,
    task_id: String,
    program: String,
    args: Vec<String>,
    cwd: String,
    envs: HashMap<String, String>,
) -> Result<(), String> {
    {
        let map = registry.0.lock().map_err(|e| e.to_string())?;
        if map.contains_key(&task_id) {
            return Err(format!("任务 {task_id} 正在运行，请先等待或停止"));
        }
    }

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .current_dir(&cwd)
        .envs(&envs)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW：release 下不闪黑窗
        cmd.creation_flags(0x0800_0000);
    }

    let mut child = cmd.spawn().map_err(|e| format!("启动失败（program={program}）：{e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let app_out = app.clone();
    let tid_out = task_id.clone();
    thread::spawn(move || {
        if let Some(out) = stdout {
            pump_lines(out, move |line| {
                let _ = app_out.emit(
                    "task-line",
                    TaskLine { task_id: tid_out.clone(), stream: "stdout".into(), line },
                );
            });
        }
    });

    let app_err = app.clone();
    let tid_err = task_id.clone();
    thread::spawn(move || {
        if let Some(err) = stderr {
            pump_lines(err, move |line| {
                let _ = app_err.emit(
                    "task-line",
                    TaskLine { task_id: tid_err.clone(), stream: "stderr".into(), line },
                );
            });
        }
    });

    registry
        .0
        .lock()
        .map_err(|e| e.to_string())?
        .insert(task_id.clone(), child);

    // 轮询退出状态（child 留在注册表里，kill 才有句柄；退出后移除并发 task-exit）
    let app_watch = app;
    thread::spawn(move || {
        let registry = app_watch.state::<TaskRegistry>();
        loop {
            thread::sleep(Duration::from_millis(300));
            let mut map = match registry.0.lock() {
                Ok(m) => m,
                Err(_) => return,
            };
            let done = match map.get_mut(&task_id) {
                Some(child) => match child.try_wait() {
                    Ok(Some(status)) => Some(status.code()),
                    Ok(None) => None,
                    Err(_) => Some(None),
                },
                // 已被移除（不会发生，但防御一下）
                None => return,
            };
            if let Some(code) = done {
                map.remove(&task_id);
                drop(map);
                let _ = app_watch.emit(
                    "task-exit",
                    TaskExit { task_id, code, success: code == Some(0) },
                );
                return;
            }
        }
    });

    Ok(())
}

/// 终止一个运行中的任务（kill -9 语义；退出事件随后由 watcher 线程发出，code 为 None）。
#[tauri::command]
pub fn stop_task(registry: State<'_, TaskRegistry>, task_id: String) -> Result<(), String> {
    let mut map = registry.0.lock().map_err(|e| e.to_string())?;
    match map.get_mut(&task_id) {
        Some(child) => {
            let _ = child.kill();
            Ok(())
        }
        None => Err(format!("任务 {task_id} 不存在或已结束")),
    }
}

/// 确保安卓 release 签名密钥库存在：不存在则用 `keytool` 自动生成自签名密钥库。
///
/// RNbase 的 `app/build.gradle` 只在 gradle 属性 `QUX_UPLOAD_STORE_FILE` 存在时才填
/// release 签名配置；该属性由环境变量 `ORG_GRADLE_PROJECT_QUX_UPLOAD_*` 注入
/// （见 platforms/android/gradle.properties 注释）。密钥库文件（`*.keystore`）被 gitignore，
/// **不会随仓库同步** —— 换一台机器打正式包必然缺它，从而导致
/// `SigningConfig "release" is missing required property "storeFile"`。
///
/// 这里把"缺则自动生成"的能力收进打包器：首次在任意机器上打正式包时，
/// 前端会先调用本命令保证密钥库就位，再把签名变量注入 gradle 进程，
/// 从而让 release 构建开箱即用（除非用户在设置里自带了 `QUX_UPLOAD_*`，则尊重其配置）。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeystoreResult {
    /// 密钥库绝对路径
    pub path: String,
    /// 调用前文件已存在（复用既有密钥库，未重新生成）
    pub existed: bool,
    /// 本次调用实际新生成了密钥库
    pub created: bool,
}

#[tauri::command]
pub fn ensure_release_keystore(
    android_app_dir: String,
    keystore_name: String,
    alias: String,
    store_password: String,
    key_password: String,
    envs: HashMap<String, String>,
) -> Result<KeystoreResult, String> {
    let dir = Path::new(&android_app_dir);
    let path = dir.join(&keystore_name);

    if path.exists() {
        return Ok(KeystoreResult {
            path: path.to_string_lossy().into_owned(),
            existed: true,
            created: false,
        });
    }

    // 定位 keytool：优先 JAVA_HOME/bin，否则 PATH 上的 keytool
    let keytool = locate_keytool(&envs);

    std::fs::create_dir_all(dir).map_err(|e| format!("创建目录失败 {android_app_dir}：{e}"))?;

    // -dname 提供完整 DN，避免 keytool 进入交互式逐项提问（后台进程会卡死等待输入）。
    let dname = "CN=QUX, OU=Dev, O=QUX, L=Shenzhen, ST=Guangdong, C=CN";
    let status = Command::new(&keytool)
        .args([
            "-genkeypair",
            "-v",
            "-keystore",
            &path.to_string_lossy(),
            "-keyalg",
            "RSA",
            "-keysize",
            "2048",
            "-validity",
            "10000",
            "-alias",
            &alias,
            "-storepass",
            &store_password,
            "-keypass",
            &key_password,
            "-dname",
            dname,
        ])
        .envs(&envs)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .status()
        .map_err(|e| {
            format!(
                "启动 keytool 失败（program={keytool:?}）：{e}。请确认 JAVA_HOME 指向 JDK，或 keytool 已在 PATH 中"
            )
        })?;

    if !status.success() {
        return Err(format!(
            "生成签名密钥库失败（keytool 退出码 {status:?}）。请检查 JAVA_HOME / JDK 配置，或在设置中改用自有密钥库"
        ));
    }

    Ok(KeystoreResult {
        path: path.to_string_lossy().into_owned(),
        existed: false,
        created: true,
    })
}

/// 定位 keytool：优先 `JAVA_HOME/bin`，否则 PATH 上的 keytool。
fn locate_keytool(envs: &HashMap<String, String>) -> PathBuf {
    let java_home = envs.get("JAVA_HOME").filter(|v| !v.trim().is_empty());
    if let Some(home) = java_home {
        let p = Path::new(home)
            .join("bin")
            .join(if cfg!(windows) { "keytool.exe" } else { "keytool" });
        if p.exists() {
            return p;
        }
    }
    Path::new("keytool").to_path_buf()
}

/// 校验安卓 release 签名密钥库：密钥库是否存在、密钥库口令(storepass)是否正确、
/// 别名是否存在、密钥口令(keypass)是否正确。用于在正式包构建**前**就发现
/// 「口令不匹配 / 别名不符」这类错误（否则 gradle 会跑到 packageRelease 才抛
/// `KeytoolException: keystore password was incorrect`，排查成本高）。
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeystoreVerify {
    /// 整体是否可用（四项全通过）
    pub ok: bool,
    /// 密钥库文件是否存在
    pub exists: bool,
    /// 密钥库口令(storepass)是否正确
    pub store_ok: bool,
    /// 别名是否存在于密钥库中
    pub alias_exists: bool,
    /// 密钥口令(keypass)是否正确（仅当 store_ok 且 alias_exists 时才有意义）
    pub key_ok: bool,
    /// 人类可读诊断信息
    pub message: String,
}

#[tauri::command]
pub fn verify_release_keystore(
    android_app_dir: String,
    keystore_name: String,
    alias: String,
    store_password: String,
    key_password: String,
    envs: HashMap<String, String>,
) -> Result<KeystoreVerify, String> {
    let dir = Path::new(&android_app_dir);
    let path = dir.join(&keystore_name);
    if !path.exists() {
        return Ok(KeystoreVerify {
            ok: false,
            exists: false,
            store_ok: false,
            alias_exists: false,
            key_ok: false,
            message: format!(
                "密钥库不存在：{}。请先在设置页生成，或确认文件名与基座路径正确",
                path.to_string_lossy()
            ),
        });
    }
    let keytool = locate_keytool(&envs);

    // 1) 校验密钥库口令：只列库（不需要 keypass / alias）
    let out1 = Command::new(&keytool)
        .args([
            "-list",
            "-keystore",
            &path.to_string_lossy(),
            "-storepass",
            &store_password,
        ])
        .envs(&envs)
        .output()
        .map_err(|e| format!("启动 keytool 失败（{keytool:?}）：{e}"))?;
    let s1 = String::from_utf8_lossy(&out1.stderr).to_string()
        + &String::from_utf8_lossy(&out1.stdout);
    if !out1.status.success() {
        if s1.contains("password was incorrect")
            || s1.contains("Password verification failed")
            || s1.contains("tampered")
        {
            return Ok(KeystoreVerify {
                ok: false,
                exists: true,
                store_ok: false,
                alias_exists: false,
                key_ok: false,
                message: "密钥库口令(storepass)不正确：请在设置页填写生成密钥库时使用的密钥库口令".into(),
            });
        }
        return Err(format!(
            "读取密钥库失败：{}",
            s1.trim().lines().last().unwrap_or("未知错误").to_string()
        ));
    }

    // 2) 校验别名 + 密钥口令
    let out2 = Command::new(&keytool)
        .args([
            "-list",
            "-keystore",
            &path.to_string_lossy(),
            "-storepass",
            &store_password,
            "-keypass",
            &key_password,
            "-alias",
            &alias,
        ])
        .envs(&envs)
        .output()
        .map_err(|e| format!("启动 keytool 失败（{keytool:?}）：{e}"))?;
    let s2 = String::from_utf8_lossy(&out2.stderr).to_string()
        + &String::from_utf8_lossy(&out2.stdout);
    if !out2.status.success() {
        if s2.contains("alias") && s2.contains("does not exist") {
            return Ok(KeystoreVerify {
                ok: false,
                exists: true,
                store_ok: true,
                alias_exists: false,
                key_ok: false,
                message: format!(
                    "密钥库口令正确，但别名「{alias}」不存在：请在设置页填写生成密钥库时使用的别名"
                ),
            });
        }
        if s2.contains("Cannot recover key")
            || s2.contains("UnrecoverableKeyException")
            || s2.contains("password was incorrect")
        {
            return Ok(KeystoreVerify {
                ok: false,
                exists: true,
                store_ok: true,
                alias_exists: true,
                key_ok: false,
                message: "密钥口令(keypass)不正确：请在设置页填写生成密钥库时使用的密钥口令（注意密钥口令可能与密钥库口令不同）".into(),
            });
        }
        return Err(format!(
            "校验密钥失败：{}",
            s2.trim().lines().last().unwrap_or("未知错误").to_string()
        ));
    }

    Ok(KeystoreVerify {
        ok: true,
        exists: true,
        store_ok: true,
        alias_exists: true,
        key_ok: true,
        message: "密钥库校验通过：口令与别名均正确".into(),
    })
}
