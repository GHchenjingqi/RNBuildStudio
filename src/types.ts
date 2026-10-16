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

/** scan_subapps 发现的同级子应用目录（含 app.config.json 的目录） */
export interface DiscoveredSubapp {
  /** 目录名，作为注册表键名 */
  name: string;
  /** 目录绝对路径 */
  dir: string;
  /** app.config.json 里的应用名（展示用，可能为空） */
  appName: string;
  /** app.config.json 里的包名（展示用，可能为空） */
  applicationId: string;
  /** 推断出的产物目录名（dist-file / dist） */
  dist: string;
  /** 该目录是否已存在构建产物（dist/index.html 或 dist-file/index.html） */
  built: boolean;
  /** 是否已登记进 app-registry.json */
  registered: boolean;
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

/** 安卓 release 签名配置（本地密钥库，落在本机，不随仓库同步） */
export interface SigningConfig {
  /** 密钥库文件名，落在 <基座>/platforms/android/app/ 下（与 app/build.gradle 相对解析一致） */
  keystoreName: string;
  /** 密钥别名 */
  alias: string;
  /** 密钥库口令 */
  storePassword: string;
  /** 密钥口令（通常与密钥库口令相同） */
  keyPassword: string;
}

/** 工作台设置（localStorage 持久化） */
export interface Settings {
  /** RNbase 基座路径 */
  baseDir: string;
  /** node 可执行文件路径（默认 node，双击 exe 启动时可能需要绝对路径） */
  nodePath: string;
  /** 工具链环境变量注入（GUI 启动不继承 shell 的 PATH） */
  env: Record<string, string>;
  /** 安卓正式包签名（缺密钥库时打包器自动生成，也可改为自有密钥库） */
  signing: SigningConfig;
}

export const DEFAULT_SETTINGS: Settings = {
  baseDir: 'D:/codes/RNbase',
  nodePath: 'node',
  env: {},
  signing: {
    keystoreName: 'QUX-release.keystore',
    alias: 'qux',
    storePassword: 'android',
    keyPassword: 'android',
  },
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
