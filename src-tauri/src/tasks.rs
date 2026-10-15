//! 任务编排：spawn 基座脚本（node scripts/xxx.js），stdout/stderr 逐行经事件推给前端。
//!
//! 设计约束（来自 RNbase 打包链路的实际行为）：
//!  - 全局单并发：gradle 与 hvigor 不能同时跑（会 OOM），前端用任务锁保证一次一个；
//!  - hdc 永远返回 0，成功与否只能解析输出，所以 stdout/stderr 都要完整转发到前端；
//!  - 双击 exe 启动时不继承 shell 的 PATH，工具链路径由前端设置页注入 envs。

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
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
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                let _ = app_out.emit(
                    "task-line",
                    TaskLine { task_id: tid_out.clone(), stream: "stdout".into(), line },
                );
            }
        }
    });

    let app_err = app.clone();
    let tid_err = task_id.clone();
    thread::spawn(move || {
        if let Some(err) = stderr {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                let _ = app_err.emit(
                    "task-line",
                    TaskLine { task_id: tid_err.clone(), stream: "stderr".into(), line },
                );
            }
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
