import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AppConfig, AppRegistry, TaskExit, TaskLine } from '../types';

export interface RunTaskOptions {
  [key: string]: unknown;
  taskId: string;
  program: string;
  args: string[];
  cwd: string;
  envs: Record<string, string>;
}

export const api = {
  /** 启动基座脚本任务（日志走 task-line / task-exit 事件流） */
  runTask: (opts: RunTaskOptions) => invoke<void>('run_task', opts),
  stopTask: (taskId: string) => invoke<void>('stop_task', { taskId }),
  listApps: (baseDir: string) => invoke<AppRegistry>('list_apps', { baseDir }),
  /** 注销注册表中的一条记录（清理目录已被删掉的应用） */
  unregisterApp: (baseDir: string, appName: string) =>
    invoke<void>('unregister_app', { baseDir, appName }),
  readAppConfig: (baseDir: string, appName: string) =>
    invoke<AppConfig>('read_app_config', { baseDir, appName }),
  writeAppConfig: (baseDir: string, appName: string, config: AppConfig) =>
    invoke<void>('write_app_config', { baseDir, appName, config }),
  /** 自动探测 RNbase 基座目录（exe 在 RNbase 根目录即认出） */
  detectBaseDir: () => invoke<string>('detect_base_dir'),
  /** 校验目录是否为合法基座 */
  validateBaseDir: (path: string) => invoke<boolean>('validate_base_dir', { path }),
  onTaskLine: (cb: (l: TaskLine) => void): Promise<UnlistenFn> =>
    listen<TaskLine>('task-line', (e) => cb(e.payload)),
  onTaskExit: (cb: (e: TaskExit) => void): Promise<UnlistenFn> =>
    listen<TaskExit>('task-exit', (e) => cb(e.payload)),
};
