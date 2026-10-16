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

/** ensure_release_keystore 入参 */
export interface EnsureKeystoreOptions {
  [key: string]: unknown;
  androidAppDir: string;
  keystoreName: string;
  alias: string;
  storePassword: string;
  keyPassword: string;
  envs: Record<string, string>;
}

/** ensure_release_keystore 返回 */
export interface KeystoreResult {
  path: string;
  existed: boolean;
  created: boolean;
}

/** verify_release_keystore 入参（与 EnsureKeystoreOptions 同构） */
export interface VerifyKeystoreOptions {
  [key: string]: unknown;
  androidAppDir: string;
  keystoreName: string;
  alias: string;
  storePassword: string;
  keyPassword: string;
  envs: Record<string, string>;
}

/** verify_release_keystore 返回 */
export interface KeystoreVerify {
  ok: boolean;
  exists: boolean;
  storeOk: boolean;
  aliasExists: boolean;
  keyOk: boolean;
  message: string;
}

export const api = {
  /** 启动基座脚本任务（日志走 task-line / task-exit 事件流） */
  runTask: (opts: RunTaskOptions) => invoke<void>('run_task', opts),
  stopTask: (taskId: string) => invoke<void>('stop_task', { taskId }),
  /** 确保安卓 release 签名密钥库存在（缺则自动用 keytool 生成），返回路径与状态 */
  ensureReleaseKeystore: (opts: EnsureKeystoreOptions) =>
    invoke<KeystoreResult>('ensure_release_keystore', opts),
  /** 校验密钥库口令/别名是否正确（构建前预检，避免 gradle 跑一半才报 KeytoolException） */
  verifyReleaseKeystore: (opts: VerifyKeystoreOptions) =>
    invoke<KeystoreVerify>('verify_release_keystore', opts),
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
