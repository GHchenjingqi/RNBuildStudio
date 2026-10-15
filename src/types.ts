/** list_apps 实时算出的体检结果（不落在注册表文件里） */
export interface AppHealth {
  /** 注册表里的 dir 解析成的绝对路径 */
  dir: string;
  dirExists: boolean;
  configExists: boolean;
  /** dir 与 app.config.json 都在，才是一个可用（可选中）的应用 */
  ok: boolean;
}

export interface AppRegistry {
  activeApp: string;
  apps: Record<string, { dir: string; dist: string }>;
  /** 由 list_apps 附带回传，读不到就按「全部可用」处理（老版本后端） */
  health?: Record<string, AppHealth>;
}

/** 子应用 app.config.json（版本与品牌唯一真源），与基座 apply-app-meta.js 字段对齐 */
export interface AppConfig {
  appName: string;
  apkName: string;
  applicationId: string;
  versionName: string;
  versionCode: number;
  splashBackgroundColor: string;
  remoteUrl?: string;
  statusBarStyle?: string;
  [key: string]: unknown;
}

export interface TaskLine {
  taskId: string;
  stream: 'stdout' | 'stderr';
  line: string;
}

export interface TaskExit {
  taskId: string;
  code: number | null;
  success: boolean;
}

/** 工作台设置（localStorage 持久化） */
export interface Settings {
  /** RNbase 基座路径 */
  baseDir: string;
  /** node 可执行文件路径（默认 node，双击 exe 启动时可能需要绝对路径） */
  nodePath: string;
  /** 工具链环境变量注入（GUI 启动不继承 shell 的 PATH） */
  env: Record<string, string>;
}

export const DEFAULT_SETTINGS: Settings = {
  baseDir: 'D:/codes/RNbase',
  nodePath: 'node',
  env: {},
};

/** 设置页预置的工具链环境变量（可留空）。key 为真实环境变量名（必须英文），
 *  desc 为中文释义，给不懂英文的用户看。 */
export const ENV_PRESETS: { key: string; desc: string }[] = [
  { key: 'JAVA_HOME', desc: 'JDK 安装目录（安卓 / 鸿蒙打包需要）' },
  { key: 'ANDROID_HOME', desc: 'Android SDK 安装目录' },
  { key: 'DEVECO_HOME', desc: 'DevEco Studio 安装目录（鸿蒙打包需要）' },
  { key: 'HOS_HDC', desc: '鸿蒙 hdc 命令行工具路径' },
  { key: 'HOS_EMULATOR', desc: '鸿蒙模拟器路径（可选）' },
  { key: 'NODE_HOME', desc: 'Node 安装目录（可选，exe 自带时可留空）' },
];
