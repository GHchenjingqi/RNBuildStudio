<script setup lang="ts">
import { computed, ref, watch, onUnmounted } from 'vue';
import { ElMessage } from 'element-plus';
import { convertFileSrc } from '@tauri-apps/api/core';
import { settings } from '../lib/store';
import { taskRunner, useTaskView } from '../lib/taskRunner';
import { api, type BrandImportResult } from '../lib/tauri';
import type { AppConfig, AppHealth, DiscoveredSubapp } from '../types';

const step = ref(0);

// ── 步骤 1：选平台（MVP 只放行安卓）──────────────────────────
const platform = ref<'android' | 'harmony'>('android');

/** 正式包 / 调试包：决定传给基座构建脚本的参数。 */
const release = ref(false);

/**
 * 当前平台对应的构建参数。
 * 与基座 package.json 的 npm scripts 一一对应，GUI 不发明新命令：
 *   build:android         -> scripts/gradle.js assembleDebug
 *   build:android:release -> scripts/gradle.js assembleRelease
 *   build:harmony         -> scripts/hvigor.js assembleHap
 *   build:harmony:release -> scripts/hvigor.js assembleHap --release
 */
function buildArgs(): string[] {
  return platform.value === 'harmony'
    ? release.value
      ? ['scripts/hvigor.js', 'assembleHap', '--release']
      : ['scripts/hvigor.js', 'assembleHap']
    : release.value
      ? ['scripts/gradle.js', 'assembleRelease']
      : ['scripts/gradle.js', 'assembleDebug'];
}

/**
 * 装机参数：安卓 installDebug，鸿蒙 assembleHap --install。
 * 注意鸿蒙的 hdc 无论成功失败都返回 0，成功与否只能解析输出里的
 * [Fail] / [Empty] 关键字（hvigor.js 已处理），这里不自行判断 exit code。
 */
function installArgs(): string[] {
  return platform.value === 'harmony'
    ? ['scripts/hvigor.js', 'assembleHap', '--install']
    : ['scripts/gradle.js', 'installDebug'];
}

// ── 步骤 2：选模式 + 参数表单 ────────────────────────────────
// 实际只有两种场景：① 产物已经在基座 h5/ 里（现成的 H5，或手动把子应用构建好同步进去）
// → 直接打包 h5/；② 是个网址 → 远程站点。原先的「本地子应用」模式与「直接用 h5/ 打包」
// 重复（都要先把产物弄进 h5/），已移除。
// adhoc  = 直接用 h5/ 打包（免子应用：向导内联手填品牌信息，APK 加载 h5/）
// remote = 远程站点（打包成壳，打开时加载网址）
const mode = ref<'adhoc' | 'remote'>('adhoc');
const apps = ref<string[]>([]);
const health = ref<Record<string, AppHealth>>({});
const activeApp = ref('');
const selectedApp = ref('');
const loadingApps = ref(false);
const saving = ref(false);
const removing = ref('');

/** 目录与 app.config.json 都在才算可用；空值一律算不可用。
 *  拿不到体检结果（老版本后端）时按可用处理，避免整个列表变灰。 */
function usable(name: string): boolean {
  if (!name) return false;
  const h = health.value[name];
  return h ? h.ok : true;
}

const usableApps = computed(() => apps.value.filter(usable));
/** 悬空记录：目录被删/改名，或里面没有 app.config.json */
const brokenApps = computed(() => apps.value.filter((n) => !usable(n)));

function labelOf(name: string): string {
  const suffix = name === activeApp.value ? '（当前活动）' : '';
  return usable(name) ? name + suffix : `${name}（不可用：目录已丢失）${suffix}`;
}

const applyTaskId = 'apply-brand';
const { lines: applyLines, running: applyRunning, exit: applyExit } = useTaskView(ref(applyTaskId));

const form = ref<AppConfig>({
  appName: '',
  apkName: '',
  applicationId: '',
  versionName: '',
  versionCode: 1,
  splashBackgroundColor: '#08080D',
  remoteUrl: '',
  statusBarStyle: '',
});

/**
 * 读取注册表 + 体检结果。
 * 关键点：activeApp 可能指向一个已经被删掉目录的应用（用户删了目录又重建），
 * 所以这里不能无条件沿用 activeApp，必须挑第一个真正可用的应用。
 */
async function loadApps() {
  loadingApps.value = true;
  try {
    const reg = await api.listApps(settings.value.baseDir);
    health.value = reg.health ?? {};
    apps.value = Object.keys(reg.apps ?? {});
    activeApp.value = reg.activeApp ?? '';
    if (!selectedApp.value || !usable(selectedApp.value)) {
      const preferActive = Boolean(activeApp.value) && usable(activeApp.value);
      selectedApp.value = preferActive ? activeApp.value : usableApps.value[0] ?? '';
    }
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    loadingApps.value = false;
  }
}

watch(
  () => [step.value, mode.value],
  async ([s]) => {
    // 每次进入第 2 步都重读：用户在「新建子应用」页刚创建的应用要能立刻出现在列表里
    if (s === 1) await loadApps();
  },
  { immediate: true }
);

/** 把目录已经不存在的应用从注册表注销，避免每次进向导都在列表里留一条死记录。 */
async function unregister(name: string) {
  removing.value = name;
  try {
    await api.unregisterApp(settings.value.baseDir, name);
    ElMessage.success(`已把「${name}」从注册表移除`);
    apps.value = [];
    await loadApps();
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    removing.value = '';
  }
}

// ── 选择已有子应用目录：扫描基座同级目录，登记后选它出包 ────────────────
const pickVisible = ref(false);
const scanning = ref(false);
const discovered = ref<DiscoveredSubapp[]>([]);

/** 扫描基座同级目录里含 app.config.json 的子应用目录 */
async function openPickDir() {
  pickVisible.value = true;
  scanning.value = true;
  discovered.value = [];
  try {
    discovered.value = await api.scanSubapps(settings.value.baseDir);
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    scanning.value = false;
  }
}

/** 仅登记：写进注册表，刷新列表并选中，关闭弹窗 */
async function registerOnly(d: DiscoveredSubapp) {
  try {
    await api.registerApp(settings.value.baseDir, d.name, d.dir, d.dist);
    ElMessage.success(`已登记「${d.name}」`);
    await loadApps();
    selectedApp.value = d.name;
    pickVisible.value = false;
  } catch (e) {
    ElMessage.error(String(e));
  }
}

/** 已登记的目录：直接选它为当前打包目标（用匹配的注册表键名，键名可能≠目录名） */
function chooseRegistered(d: DiscoveredSubapp) {
  selectedApp.value = d.registeredAs || d.name;
  pickVisible.value = false;
}

/** 登记 + 同步到基座（app:use --sync：清 remoteUrl + 同步 h5/ + 下发品牌） */
async function registerAndSync(d: DiscoveredSubapp) {
  try {
    await api.registerApp(settings.value.baseDir, d.name, d.dir, d.dist);
  } catch (e) {
    ElMessage.error(String(e));
    return;
  }
  await runUseSync(d.name);
}

/** 已登记目录：直接同步到基座（用匹配的注册表键名） */
async function syncExisting(d: DiscoveredSubapp) {
  await runUseSync(d.registeredAs || d.name);
}

/** 跑 use-app.js <name> --sync：把该子应用产物同步进基座 h5/ 并设为当前活动应用 */
const pickSyncTaskId = 'pick-sync';
const { running: pickSyncRunning, exit: pickSyncExit } = useTaskView(ref(pickSyncTaskId));
watch(pickSyncExit, (e) => {
  if (!e) return;
  if (e.success) {
    ElMessage.success('已同步到基座 h5/，并已设为当前活动应用，可直接出包');
    loadApps();
    pickVisible.value = false;
  } else {
    ElMessage.error(`同步失败（exit ${e.code ?? '已终止'}），请检查该目录是否已构建（npm run build）`);
  }
});
async function runUseSync(name: string) {
  await taskRunner.start(pickSyncTaskId, {
    program: settings.value.nodePath,
    args: ['scripts/use-app.js', name, '--sync'],
    cwd: settings.value.baseDir,
    envs: settings.value.env,
  });
}

// ── 「直接用 h5/ 打包」模式：免子应用，向导内联手填品牌身份 ────────────────
const isAdhoc = computed(() => mode.value === 'adhoc');
const adhocPhase = ref<'idle' | 'apply'>('idle');
const adhocApplyTaskId = 'adhoc-apply';
const { running: adhocApplyRunning, exit: adhocApplyExit } = useTaskView(ref(adhocApplyTaskId));
const adhocBusy = computed(
  () => adhocPhase.value !== 'idle' || adhocApplyRunning.value || brandGenRunning.value,
);

// ── 品牌图（图标 / 闪屏主图）：两种模式通用 ─────────────────
// 留空 = 不改动（子应用沿用已有品牌，免子应用沿用基座默认图标）。
// 选取走原生文件对话框：WebView 的 <input type=file> 出于安全拿不到真实路径，
// 而切图脚本与缩略图预览都需要绝对路径。
const brandIcon = ref('');
const brandSplash = ref('');
/** 闪屏主图必须与图标主图一起给：gen-brand 的 --src（图标）是必填主图，缺它无法切图 */
const brandValid = computed(() => !(brandSplash.value.trim() && !brandIcon.value.trim()));
const brandIconSrc = computed(() =>
  brandIcon.value.trim() ? convertFileSrc(brandIcon.value.trim()) : '',
);
const brandSplashSrc = computed(() =>
  brandSplash.value.trim() ? convertFileSrc(brandSplash.value.trim()) : '',
);

async function pickBrandImage(kind: 'icon' | 'splash') {
  const picked = await api.pickImage(
    kind === 'icon' ? '选择图标主图（方形 PNG，≥1024）' : '选择闪屏主图（PNG）',
  );
  if (!picked) return;
  if (kind === 'icon') brandIcon.value = picked;
  else brandSplash.value = picked;
}

function clearBrandImage(kind: 'icon' | 'splash') {
  if (kind === 'icon') brandIcon.value = '';
  else brandSplash.value = '';
}

/** 品牌切图任务：主图导入 app-brand/ 后跑基座 scripts/gen-brand.js 切五档密度。
 *  真源（icon.png / splash.png）与产物（app-brand/android/）都落在应用目录里，
 *  之后用户仍可手工 npm run brand:gen 重切，与 CLI 完全同源。 */
const brandGenTaskId = 'brand-gen';
const { running: brandGenRunning, exit: brandGenExit } = useTaskView(ref(brandGenTaskId));

/** 切图成功后的续跑动作：adhoc → 下发临时品牌；subapp → use-app.js 切活动 + 下发 */
type PendingAfterGen = { kind: 'adhoc'; slug: string } | { kind: 'subapp' } | null;
const pendingAfterGen = ref<PendingAfterGen>(null);

function brandGenArgs(dir: string, withSplash: boolean): string[] {
  return [
    'scripts/gen-brand.js',
    '--src',
    `${dir}/app-brand/icon.png`,
    ...(withSplash ? ['--splash', `${dir}/app-brand/splash.png`] : []),
    '--out',
    `${dir}/app-brand/android`,
  ];
}

/** 导入主图（本地快速拷贝，直接 await）→ 启动切图任务；续跑交给 brandGenExit watcher。 */
async function importAndGen(dir: string, pending: Exclude<PendingAfterGen, null>): Promise<boolean> {
  let imported: BrandImportResult;
  try {
    imported = await api.importBrandImages(dir, brandIcon.value.trim(), brandSplash.value.trim());
  } catch (e) {
    ElMessage.error(String(e));
    return false;
  }
  pendingAfterGen.value = pending;
  await taskRunner.start(brandGenTaskId, {
    program: settings.value.nodePath,
    args: brandGenArgs(dir, imported.splash),
    cwd: settings.value.baseDir,
    envs: settings.value.env,
  });
  return true;
}

watch(brandGenExit, (e) => {
  const pending = pendingAfterGen.value;
  if (!e || !pending) return;
  pendingAfterGen.value = null;
  if (!e.success) {
    ElMessage.error('品牌图切密度失败（gen-brand）——请确认主图是 8bit 非隔行 PNG，图标主图需为方形');
    adhocPhase.value = 'idle';
    return;
  }
  if (pending.kind === 'adhoc') void runAdhocApply(pending.slug);
  else void runApplyTask();
});

/** 临时配置目录名（.studio-adhoc/<slug>）：仅用安装包名收敛成安全字符。 */
function adhocSlug(): string {
  const s = (form.value.apkName || form.value.applicationId || 'adhoc').replace(
    /[^A-Za-z0-9_-]/g,
    '_',
  );
  return s || 'adhoc';
}

/** 免子应用打包：写临时身份配置 →（可选导入品牌图并切密度）→ apply-app-meta 下发品牌。 */
async function applyAdhocBrand() {
  if (!formValid.value) {
    ElMessage.warning('请检查表单：应用名/包名不能为空，包名需为 com.xxx.yyy，版本号为 x.y.z');
    return;
  }
  if (!brandValid.value) {
    ElMessage.warning('上传闪屏主图时需同时设置图标主图');
    return;
  }
  const slug = adhocSlug();
  const cfg: AppConfig = { ...form.value, remoteUrl: '' }; // 本地 h5 模式：强制无远程网址
  let adhocDir: string;
  try {
    adhocDir = await api.writeAdhocConfig(settings.value.baseDir, slug, cfg);
  } catch (e) {
    ElMessage.error(String(e));
    return;
  }
  const wantsBrand = Boolean(brandIcon.value.trim() || brandSplash.value.trim());
  if (wantsBrand) {
    adhocPhase.value = 'apply'; // 切图期间撑住按钮 loading（adhocBusy 还算了 brandGenRunning）
    const started = await importAndGen(adhocDir, { kind: 'adhoc', slug });
    if (started) return; // 续跑（下发品牌）由 brandGenExit watcher 接力
    adhocPhase.value = 'idle';
    return;
  }
  await runAdhocApply(slug);
}

async function runAdhocApply(slug: string) {
  adhocPhase.value = 'apply';
  await taskRunner.start(adhocApplyTaskId, {
    program: settings.value.nodePath,
    args: ['scripts/apply-app-meta.js', '--app', `.studio-adhoc/${slug}`],
    cwd: settings.value.baseDir,
    envs: settings.value.env,
  });
}

watch(adhocApplyExit, (e) => {
  if (!e || adhocPhase.value !== 'apply') return;
  if (e.success) {
    ElMessage.success('品牌已按手填信息下发（临时配置）；可进入第 3 步打包，APK 将直接加载基座 h5/ 的代码');
  } else {
    ElMessage.error(`品牌下发失败（exit ${e.code ?? '已终止'}）`);
  }
  adhocPhase.value = 'idle';
});

// 进入免子应用模式时把表单复位到手填状态（不残留上一个子应用的配置）
watch(isAdhoc, (v) => {
  // 换模式就清空品牌图选择：主图是「针对某个目标应用」选的，跨模式残留会下发给错误目标
  brandIcon.value = '';
  brandSplash.value = '';
  if (v) {
    form.value = {
      appName: '',
      apkName: '',
      applicationId: '',
      versionName: '1.0.0',
      versionCode: 1,
      splashBackgroundColor: '#08080D',
      remoteUrl: '',
      statusBarStyle: '',
    };
  }
});

watch(selectedApp, async (name) => {
  if (!name || isAdhoc.value) return;
  // 换应用就清空品牌图选择，避免把上一个应用选的主图下发给新目标
  brandIcon.value = '';
  brandSplash.value = '';
  try {
    // 与默认值合并：子应用的 app.config.json 可能缺可选字段（如 statusBarStyle），
    // 直接覆盖会让表单出现 undefined。
    const cfg = await api.readAppConfig(settings.value.baseDir, name);
    form.value = { ...form.value, ...cfg };
  } catch (e) {
    ElMessage.error(String(e));
  }
});

const formValid = computed(() => {
  const f = form.value;
  return Boolean(
    (f.appName ?? '').trim() &&
      apkNameValid.value &&
      /^[a-zA-Z][a-zA-Z0-9_]*(\.[a-zA-Z_][a-zA-Z0-9_]*)+$/.test(f.applicationId) &&
      /^\d{1,3}(\.\d{1,3}){0,2}$/.test(String(f.versionName)) &&
      Number.isInteger(f.versionCode) &&
      f.versionCode >= 1 &&
      // 远程模式必须是一个合法 http(s) 地址，否则打出来的壳会指向空白页
      (mode.value !== 'remote' || /^https?:\/\/\S+$/.test(f.remoteUrl ?? ''))
  );
});

/** 安装包文件名会成为产物文件名（<apkName>-<版本>-<变体>.apk），必须是安全文件名：
 *  只允许字母数字与连字符/下划线。此前无校验，把网址粘进来会生成非法路径。 */
const apkNameValid = computed(() => /^[a-zA-Z0-9][a-zA-Z0-9_-]*$/.test(form.value.apkName));

/** 真正会被打进包的是注册表的 activeApp（gradle/hvigor 都按它解析），不是这里选中的值。 */
const selectionIsActive = computed(
  () => Boolean(selectedApp.value) && selectedApp.value === activeApp.value
);

/** 进入构建前的最后一道门：选中项必须已经是当前活动应用，否则打出来的是别人。
 *  免子应用（adhoc）模式没有活动子应用，品牌已在第 2 步按手填信息下发，故跳过此校验。 */
function nextStep() {
  if (!isAdhoc.value && !selectionIsActive.value) {
    ElMessage.warning(
      `当前打包目标是「${activeApp.value || '未设置'}」，与所选不一致；请先点「保存配置并下发品牌」切换过去`
    );
    return;
  }
  step.value = 2;
}

/** 跑 use-app.js <name>：写注册表 activeApp + 下发品牌（applyTaskId，日志走 applyLines） */
async function runApplyTask() {
  // 走 use-app.js 而不是直接 apply-app-meta.js：前者在写 busy 之前会先把 selectedApp
  // 写进注册表的 activeApp。构建脚本只认 activeApp，少了这一步就会「选了 A 却打出 B」。
  await taskRunner.start(applyTaskId, {
    program: settings.value.nodePath,
    args: ['scripts/use-app.js', selectedApp.value],
    cwd: settings.value.baseDir,
    envs: settings.value.env,
  });
  activeApp.value = selectedApp.value;
}

/** 保存 app.config.json（真源）→（可选导入品牌图并切密度）→ 切换为当前活动应用并下发品牌 */
async function saveAndApply() {
  if (!formValid.value) {
    ElMessage.warning('请检查表单：包名需为 com.xxx.yyy，版本号为 x.y.z，安装包文件名不能含网址字符');
    return;
  }
  if (!brandValid.value) {
    ElMessage.warning('上传闪屏主图时需同时设置图标主图');
    return;
  }
  saving.value = true;
  try {
    await api.writeAppConfig(settings.value.baseDir, selectedApp.value, form.value);
    const wantsBrand = Boolean(brandIcon.value.trim() || brandSplash.value.trim());
    if (wantsBrand) {
      // 应用目录绝对路径来自 list_apps 附带的体检结果；拿不到就没法定位 app-brand/
      const appDir = health.value[selectedApp.value]?.dir;
      if (!appDir) {
        ElMessage.error('拿不到该应用的目录位置，无法导入品牌图；请重新选择应用后重试');
        return;
      }
      // 主图导入 + 切密度任务已启动，后续（切活动 + 下发品牌）由 brandGenExit watcher 接力
      await importAndGen(appDir, { kind: 'subapp' });
      return;
    }
    await runApplyTask();
    ElMessage.success(`配置已保存到「${selectedApp.value}」，并已设为当前活动应用（构建将打它）`);
  } catch (e) {
    ElMessage.error(String(e));
  } finally {
    // 必须无条件复位：按钮 loading = saving || applyRunning || brandGenRunning，saving 原先
    // 只写在 catch 里，成功路径永不复位 → 明明已提示保存成功，按钮却一直转圈。
    // 任务真正在跑时由 applyRunning / brandGenRunning 撑住 loading，任务结束自动停。
    saving.value = false;
  }
}

/** 第 2 步日志面板：切图（gen-brand）与品牌下发（use-app / apply-app-meta）两段日志拼一起 */
const brandLogLines = computed(() => {
  const gen = taskRunner.text(brandGenTaskId);
  const apply = isAdhoc.value ? taskRunner.text(adhocApplyTaskId) : taskRunner.text(applyTaskId);
  const merged = [gen, apply].filter(Boolean).join('\n');
  return merged ? merged.split('\n') : [];
});

// ── 步骤 3：构建 ─────────────────────────────────────────────
const buildTaskId = 'build';
const { lines: buildLines, running: buildRunning, exit: buildExit } = useTaskView(ref(buildTaskId));
const hashTaskId = 'hash';
const { lines: hashLines, exit: hashExit } = useTaskView(ref(hashTaskId));
const hashStarted = ref(false);

const canBuild = computed(() => formValid.value);

/** 构建页提示：随平台与包类型变化，说明前置条件和产物去向。 */
const buildHint = computed(() => {
  const kind = release.value ? '正式包' : '调试包';
  const firstBuild =
    '首次在一台新机器上构建会先下载 Gradle 分发包与依赖（可能需 5–20 分钟），期间日志可能暂时无输出、并显示「正在下载」，属正常现象，请耐心等待不要关闭窗口。';
  return platform.value === 'harmony'
    ? `鸿蒙${kind}：需要本机安装 DevEco Studio（目录在「设置」页填写）；子应用模式请先执行 npm run build:base；产物投递到基座 dist/<包名>/。${firstBuild}`
    : `安卓${kind}：正式包前请确认 Metro 已停（npm run stop）；子应用模式请先执行 npm run build:base；产物投递到基座 dist/<包名>/，构建后自动计算 SHA-256。${firstBuild}`;
});

async function build() {
  hashStarted.value = false;
  // 合并工具链环境变量；release 包额外注入签名变量（缺密钥库则打包器自动生成）
  const envs: Record<string, string> = { ...settings.value.env };
  if (release.value) {
    const userStoreFile = settings.value.env['ORG_GRADLE_PROJECT_QUX_UPLOAD_STORE_FILE'];
    if (userStoreFile) {
      ElMessage.info('检测到设置中自定义的 release 签名变量，将直接使用');
    } else {
      const androidAppDir = `${settings.value.baseDir}/platforms/android/app`;
      const r = await api.ensureReleaseKeystore({
        androidAppDir,
        keystoreName: settings.value.signing.keystoreName,
        alias: settings.value.signing.alias,
        storePassword: settings.value.signing.storePassword,
        keyPassword: settings.value.signing.keyPassword,
        envs: settings.value.env,
      });
      const s = settings.value.signing;
      // 构建前预校验口令/别名，避免 gradle 跑到 packageRelease 才抛 KeytoolException
      const v = await api.verifyReleaseKeystore({
        androidAppDir,
        keystoreName: s.keystoreName,
        alias: s.alias,
        storePassword: s.storePassword,
        keyPassword: s.keyPassword,
        envs: settings.value.env,
      });
      if (!v.ok) {
        ElMessage.error(`签名校验未通过，已中止构建：${v.message}`);
        return;
      }
      envs[`ORG_GRADLE_PROJECT_QUX_UPLOAD_STORE_FILE`] = s.keystoreName;
      envs[`ORG_GRADLE_PROJECT_QUX_UPLOAD_STORE_PASSWORD`] = s.storePassword;
      envs[`ORG_GRADLE_PROJECT_QUX_UPLOAD_KEY_ALIAS`] = s.alias;
      envs[`ORG_GRADLE_PROJECT_QUX_UPLOAD_KEY_PASSWORD`] = s.keyPassword;
      ElMessage.success(
        `release 签名密钥库${r.existed ? '已复用' : '已自动生成'}且校验通过：${s.keystoreName}（生产发布请备份此文件）`,
      );
    }
  }
  await taskRunner.start(buildTaskId, {
    program: settings.value.nodePath,
    args: buildArgs(),
    cwd: settings.value.baseDir,
    envs,
  });
}

const installTaskId = 'install';
const { lines: installLines, running: installRunning } = useTaskView(ref(installTaskId));

/** 装机到已连接的设备 / 模拟器（安卓走 adb，鸿蒙走 hdc）。 */
async function install() {
  await taskRunner.start(installTaskId, {
    program: settings.value.nodePath,
    args: installArgs(),
    cwd: settings.value.baseDir,
    envs: settings.value.env,
  });
}

// ── 构建静默看门狗：gradle 下载分发包 / 依赖时几乎无输出，UI 像卡死。
//    超过 8 秒没新行就提示「正在下载 / 编译」，并尽量把下载百分比显示出来，
//    顺便在构建失败时把原因显眼地提示出来（原因其实实时写进了 RNbase/log/build.log）。 ──
const buildStallMsg = ref('');
const buildLogPath = computed(() => `${settings.value.baseDir}/log/build.log`);
let stallTimer: ReturnType<typeof setTimeout> | null = null;

/** 最近日志里出现的最大下载百分比（gradlew 用 \r 刷进度，后端已逐帧转发成行）。 */
const downloadPct = computed<number | null>(() => {
  const recent = buildLines.value.slice(-40).join('\n');
  if (!/download|gradle|wrapper|分发包|distribution/i.test(recent)) return null;
  const nums = [...recent.matchAll(/(\d{1,3})\s*%/g)].map((m) => Number(m[1]));
  return nums.length ? Math.max(...nums) : null;
});

function refreshStallWatch() {
  buildStallMsg.value = '';
  if (stallTimer) {
    clearTimeout(stallTimer);
    stallTimer = null;
  }
  if (buildRunning.value) {
    stallTimer = setTimeout(() => {
      buildStallMsg.value =
        downloadPct.value != null
          ? `正在下载工具链（约 ${downloadPct.value}%，首次构建通常需 5–20 分钟），请耐心等待，不要关闭窗口`
          : '构建进行中：可能正在下载 Gradle 分发包 / 依赖或编译原生代码（首次构建通常需 5–20 分钟），期间日志可能暂时无输出，属正常现象，请耐心等待';
    }, 8000);
  }
}

watch(() => buildLines.value.length, () => refreshStallWatch());
watch(buildRunning, (r) => {
  if (r) refreshStallWatch();
  else {
    if (stallTimer) clearTimeout(stallTimer);
    stallTimer = null;
    buildStallMsg.value = '';
  }
});
onUnmounted(() => {
  if (stallTimer) clearTimeout(stallTimer);
});

const buildFailed = computed(() => buildExit.value != null && !buildExit.value.success);

/** 把当前构建日志复制到剪贴板，方便失败后贴给开发 / 自查，避免「没截图」 again。 */
async function copyBuildLog() {
  const text = buildLines.value.join('\n');
  try {
    await navigator.clipboard.writeText(text);
    ElMessage.success('构建日志已复制到剪贴板');
  } catch {
    ElMessage.error('复制失败（浏览器限制），请直接在下方日志面板手动选择复制');
  }
}

watch(buildExit, async (e) => {
  if (e?.success) {
    // apk-hash.js 只扫 .apk，鸿蒙产物是 .hap，不走这一步
    if (platform.value !== 'android') {
      ElMessage.success('构建成功（鸿蒙产物已投递到基座 dist/<包名>/）');
      return;
    }
    ElMessage.success('构建成功，正在计算 SHA-256…');
    hashStarted.value = true;
    await taskRunner.start(hashTaskId, {
      program: settings.value.nodePath,
      args: ['scripts/apk-hash.js'],
      cwd: settings.value.baseDir,
      envs: settings.value.env,
    });
  }
});
</script>

<template>
  <div style="padding: 20px">
    <el-steps :active="step" align-center style="margin-bottom: 24px">
      <el-step title="选择平台" />
      <el-step title="选择模式与参数" />
      <el-step title="构建" />
    </el-steps>

    <!-- 步骤 1：平台 -->
    <template v-if="step === 0">
      <el-radio-group v-model="platform" size="large">
        <el-radio-button value="android">安卓</el-radio-button>
        <el-radio-button value="harmony">鸿蒙</el-radio-button>
        <el-radio-button value="ios" disabled>iOS（需 macOS 打包机）</el-radio-button>
      </el-radio-group>
      <div class="tip" style="margin-top: 12px">
        鸿蒙打包需要本机安装 DevEco Studio，未安装请到「设置」页填写 DevEco 目录
      </div>
      <div style="margin-top: 24px">
        <el-button type="primary" @click="step = 1">下一步</el-button>
      </div>
    </template>

    <!-- 步骤 2：模式与参数 -->
    <template v-else-if="step === 1">
      <el-radio-group v-model="mode">
        <el-radio value="adhoc">直接用 h5/ 打包（免子应用）</el-radio>
        <el-radio value="remote">远程站点（打包成壳，打开时加载网址）</el-radio>
      </el-radio-group>
      <div class="tip" style="margin-top: 8px; max-width: 640px">
        {{
          mode === 'adhoc'
            ? '产物已经在基座 h5/ 里（现成的 H5，或你手动把子应用构建好同步进去），直接在这里手填应用名/包名/版本就能出包，不用建子应用、不用登记。'
            : '远程站点不需要 H5 源码：应用启动后直接加载下面填写的网址。'
        }}
      </div>

      <!-- 悬空记录：目录已被删除 / 改名，留着只会让每一步都失败（免子应用模式无关） -->
      <el-alert
        v-if="!isAdhoc && brokenApps.length"
        type="warning"
        :closable="false"
        show-icon
        style="max-width: 640px; margin: 16px 0"
      >
        <template #title>注册表里有 {{ brokenApps.length }} 条记录已经找不到目录</template>
        <div v-for="name in brokenApps" :key="name" class="broken-row">
          <span class="broken-name">{{ name }}</span>
          <span class="broken-dir">{{ health[name]?.dir ?? '' }}</span>
          <el-button
            type="danger"
            size="small"
            plain
            :loading="removing === name"
            @click="unregister(name)"
          >
            从注册表移除
          </el-button>
        </div>
        <div class="tip">目录被删掉后就留了一条悬空记录，移除它不会影响基座的任何文件</div>
      </el-alert>

      <el-alert
        v-if="!isAdhoc && !loadingApps && apps.length && !usableApps.length"
        type="error"
        :closable="false"
        show-icon
        title="没有任何可用的子应用，请先到「新建子应用」页创建一个"
        style="max-width: 640px; margin: 16px 0"
      />

      <el-form label-width="150px" style="max-width: 640px; margin-top: 20px">
        <el-form-item v-if="!isAdhoc" label="应用">
          <el-select v-model="selectedApp" :loading="loadingApps" style="width: 100%">
            <el-option
              v-for="name in apps"
              :key="name"
              :value="name"
              :label="labelOf(name)"
              :disabled="!usable(name)"
            />
          </el-select>
          <div style="margin-top: 8px">
            <el-button size="small" :disabled="taskRunner.anyRunning.value" @click="openPickDir">
              选择已有子应用目录…
            </el-button>
          </div>
          <div class="tip">
            品牌 / 版本 / 网址都存在这个应用的 app.config.json 里，这两种模式都要选一个；
            想要一个全新的应用，请到「新建子应用」页创建；已有子应用工程（在基座同级目录、
            含 app.config.json）可直接点上方「选择已有子应用目录」登记进来。
            保存配置时会同时把它设为「当前活动应用」——构建脚本只认活动应用
          </div>
        </el-form-item>
        <el-form-item label="应用名">
          <el-input v-model="form.appName" placeholder="手机上显示的名字，如「古韵诗笺」" />
        </el-form-item>
        <el-form-item label="应用包名">
          <el-input v-model="form.applicationId" placeholder="com.company.app" />
          <div class="tip">应用唯一标识，格式为 反写域名，如 com.company.app</div>
        </el-form-item>
        <el-form-item label="安装包文件名">
          <el-input v-model="form.apkName" placeholder="英文字母数字，如 GuYunShiJian" />
          <div v-if="form.apkName && !apkNameValid" class="tip-error">
            只能填字母、数字、连字符（它会成为安装包的文件名，不能填网址）
          </div>
        </el-form-item>
        <el-form-item label="版本号">
          <el-input v-model="form.versionName" placeholder="1.0.0" style="width: 160px" />
          <span style="margin-left: 12px">内部版本号</span>
          <el-input-number v-model="form.versionCode" :min="1" style="margin-left: 8px" />
        </el-form-item>
        <el-form-item label="闪屏底色">
          <el-color-picker v-model="form.splashBackgroundColor" />
          <span style="margin-left: 12px">{{ form.splashBackgroundColor }}</span>
        </el-form-item>
        <el-form-item label="桌面图标（可选）">
          <div class="brand-row">
            <img v-if="brandIconSrc" :src="brandIconSrc" class="brand-thumb" alt="图标预览" />
            <span v-else class="brand-thumb brand-thumb-empty"></span>
            <el-input
              v-model="brandIcon"
              placeholder="点「选择图片」选取 PNG 主图；留空沿用基座默认图标"
              style="flex: 1"
              clearable
            />
            <el-button :disabled="taskRunner.anyRunning.value" @click="pickBrandImage('icon')">
              选择图片…
            </el-button>
            <el-button v-if="brandIcon" text @click="clearBrandImage('icon')">清除</el-button>
          </div>
          <div class="tip">
            一张图自动生成全部尺寸：建议 ≥1024 的方形 PNG，保存配置时自动切出 5 档密度的桌面图标
            （方形圆角 + 圆形）与闪屏 logo，无需再手跑 npm run brand:gen
          </div>
        </el-form-item>
        <el-form-item label="闪屏图片（可选）">
          <div class="brand-row">
            <img v-if="brandSplashSrc" :src="brandSplashSrc" class="brand-thumb" alt="闪屏预览" />
            <span v-else class="brand-thumb brand-thumb-empty"></span>
            <el-input
              v-model="brandSplash"
              placeholder="点「选择图片」选取 PNG 主图；留空则闪屏只显示底色 + 图标 logo"
              style="flex: 1"
              clearable
            />
            <el-button :disabled="taskRunner.anyRunning.value" @click="pickBrandImage('splash')">
              选择图片…
            </el-button>
            <el-button v-if="brandSplash" text @click="clearBrandImage('splash')">清除</el-button>
          </div>
          <div class="tip">
            一张图适配所有屏幕：自动切出各密度闪屏 logo（原生启动窗口居中显示在「闪屏底色」上），
            并作为启动后 3 秒倒计时闪屏（全屏裁切显示）。方形图会做圆角处理，长方形图按比例完整显示，
            建议上传竖版长方形图；需与桌面图标一起设置
          </div>
          <div v-if="!brandValid" class="tip-error">上传闪屏图片前，请先设置上方的桌面图标主图</div>
        </el-form-item>
        <el-form-item v-if="!isAdhoc" label="远程地址（网站地址）">
          <el-input
            v-model="form.remoteUrl"
            placeholder="https://…（应用启动后直接加载这个网址）"
          />
          <div v-if="mode === 'remote'" class="tip">
            远程站点模式必填，应用启动后直接加载这个网址
          </div>
        </el-form-item>
        <el-form-item label="状态栏样式">
          <el-select v-model="form.statusBarStyle" style="width: 200px">
            <el-option label="跟随系统深浅色" value="" />
            <el-option label="深色图标（浅底）" value="dark-content" />
            <el-option label="浅色图标（深底）" value="light-content" />
          </el-select>
        </el-form-item>
      </el-form>

      <el-dialog v-model="pickVisible" title="选择已有子应用目录" width="680px">
        <div v-if="scanning" class="tip">正在扫描基座同级目录…</div>
        <el-alert
          v-else-if="!discovered.length"
          type="info"
          :closable="false"
          show-icon
          title="在基座同级目录没有发现含 app.config.json 的子应用目录"
          description="子应用需建在基座同级目录（如 D:/codes/RNbase 旁的 gushici_h5）。若你的工程在别处，请先把它放到同级目录，或到「新建子应用」页创建一个。"
        />
        <div v-else>
          <div v-for="d in discovered" :key="d.dir" class="sub-row">
            <div class="sub-head">
              <b>{{ d.name }}</b>
              <el-tag v-if="d.registered" size="small" type="success">
                已登记为 {{ d.registeredAs }}
              </el-tag>
              <el-tag v-if="d.built" size="small" type="info">已构建</el-tag>
              <span class="sub-meta">{{ d.appName }}{{ d.applicationId ? ' · ' + d.applicationId : '' }}</span>
            </div>
            <div class="sub-dir">{{ d.dir }}</div>
            <div class="sub-actions">
              <template v-if="d.registered">
                <el-button size="small" type="primary" plain @click="chooseRegistered(d)">选择</el-button>
                <el-button
                  v-if="d.built"
                  size="small"
                  :loading="pickSyncRunning"
                  :disabled="taskRunner.anyRunning.value"
                  @click="syncExisting(d)"
                >
                  同步到基座
                </el-button>
              </template>
              <template v-else>
                <el-button size="small" :disabled="taskRunner.anyRunning.value" @click="registerOnly(d)">
                  仅登记
                </el-button>
                <el-button
                  v-if="d.built"
                  size="small"
                  type="primary"
                  :loading="pickSyncRunning"
                  :disabled="taskRunner.anyRunning.value"
                  @click="registerAndSync(d)"
                >
                  登记并同步到基座
                </el-button>
                <span v-else class="tip">该目录还没有构建产物（dist/index.html），仅登记后需先构建</span>
              </template>
            </div>
          </div>
        </div>
        <template #footer>
          <el-button @click="pickVisible = false">关闭</el-button>
        </template>
      </el-dialog>

      <el-alert
        type="info"
        :closable="false"
        show-icon
        title="在上方选择图标 / 闪屏主图后，点「保存配置并下发品牌」会自动切出全部密度的桌面图标与闪屏图；不选则沿用应用现有品牌。主图与切图产物都保存在应用的 app-brand/ 目录，之后也可手工 npm run brand:gen 重切"
        style="max-width: 640px; margin-bottom: 16px"
      />

      <el-space>
        <el-button @click="step = 0">上一步</el-button>
        <el-button
          v-if="isAdhoc"
          type="primary"
          :loading="adhocBusy"
          :disabled="!canBuild"
          @click="applyAdhocBrand"
        >
          生成配置并下发品牌
        </el-button>
        <el-button
          v-else
          type="primary"
          :loading="saving || applyRunning || brandGenRunning"
          :disabled="!canBuild"
          @click="saveAndApply"
        >
          保存配置并下发品牌
        </el-button>
        <el-button
          :disabled="isAdhoc ? !adhocApplyExit?.success : !applyExit?.success"
          @click="nextStep"
        >
          下一步
        </el-button>
      </el-space>

      <el-card v-if="brandLogLines.length" shadow="never" style="margin-top: 16px">
        <template #header>{{
          isAdhoc ? '免子应用打包日志（切图 / 写临时配置 / 下发品牌）' : '切图 / 品牌下发日志'
        }}</template>
        <pre class="log">{{ brandLogLines.join('\n') }}</pre>
      </el-card>
    </template>

    <!-- 步骤 3：构建 -->
    <template v-else>
      <div style="margin-bottom: 16px">
        <span style="margin-right: 12px">包类型</span>
        <el-switch
          v-model="release"
          active-text="正式包"
          inactive-text="调试包"
          :disabled="taskRunner.anyRunning.value"
        />
        <div class="tip">
          正式包需签名：打包器会在缺少密钥库时自动生成（默认落在
          <code>platforms/android/app/{{ settings.signing.keystoreName }}</code>，可在「设置」页修改或改用自有密钥库）。
          生产发布请妥善备份该密钥库——丢失将无法再为同一应用发布更新
        </div>
      </div>
      <div class="tip" style="margin-bottom: 16px">
        本次打包目标：<b>{{ isAdhoc ? '临时配置（直接用基座 h5/）' : activeApp || '（未设置）' }}</b>
        <span v-if="!isAdhoc && selectedApp && selectedApp !== activeApp" class="tip-error">
          （与第 2 步所选「{{ selectedApp }}」不一致，请返回第 2 步重新保存配置）
        </span>
      </div>

      <el-space>
        <el-button @click="step = 1">上一步</el-button>
        <el-button
          type="primary"
          :loading="buildRunning"
          :disabled="taskRunner.anyRunning.value"
          @click="build"
        >
          {{ buildRunning ? '构建中…' : `开始构建（${release ? '正式包' : '调试包'}）` }}
        </el-button>
        <el-button
          v-if="buildRunning"
          type="danger"
          plain
          @click="taskRunner.stop(buildTaskId)"
        >
          停止
        </el-button>
        <el-button
          :loading="installRunning"
          :disabled="taskRunner.anyRunning.value || !buildExit?.success"
          @click="install"
        >
          安装到设备
        </el-button>
        <el-tag v-if="buildExit" :type="buildExit.success ? 'success' : 'danger'">
          {{ buildExit.success ? '构建成功' : `构建失败（exit ${buildExit.code ?? '已终止'}）` }}
        </el-tag>
      </el-space>

      <el-alert
        type="info"
        :closable="false"
        show-icon
        :title="buildHint"
        style="margin: 16px 0"
      />

      <!-- 静默看门狗：构建在跑但 8 秒无新行（典型是 gradle 下载分发包/依赖），提示用户没卡死 -->
      <el-alert
        v-if="buildStallMsg"
        type="warning"
        :closable="false"
        show-icon
        style="margin: 16px 0"
      >
        <template #title>{{ buildStallMsg }}</template>
        <el-progress
          :percentage="downloadPct ?? 100"
          :indeterminate="downloadPct == null"
          :show-text="downloadPct != null"
          :duration="downloadPct == null ? 3 : undefined"
          style="margin-top: 8px; max-width: 460px"
        />
      </el-alert>

      <!-- 构建失败：显眼提示，并指明完整日志已写入 log/build.log，支持一键复制 -->
      <el-alert
        v-if="buildFailed"
        type="error"
        :closable="false"
        show-icon
        style="margin: 16px 0"
      >
        <template #title>
          构建失败（exit {{ buildExit?.code ?? '已终止' }}，完整原因已写入
          <code>{{ buildLogPath }}</code>，也可点下方按钮复制当前日志贴给开发排查）
        </template>
        <el-button
          size="small"
          type="primary"
          plain
          style="margin-top: 8px"
          @click="copyBuildLog"
        >
          复制构建日志
        </el-button>
      </el-alert>

      <el-card shadow="never">
        <template #header>构建日志</template>
        <pre class="log" style="margin: 0">{{
          buildLines.join('\n').trim() || (buildRunning ? '正在执行…' : '（暂无输出）')
        }}</pre>
      </el-card>

      <el-card v-if="hashStarted" shadow="never" style="margin-top: 16px">
        <template #header>产物校验值（SHA-256，可直接粘贴进升级清单）</template>
        <pre class="log" style="margin: 0">{{ hashLines.join('\n') }}</pre>
      </el-card>

      <el-card v-if="installLines.length" shadow="never" style="margin-top: 16px">
        <template #header>装机日志</template>
        <pre class="log" style="margin: 0">{{ installLines.join('\n') }}</pre>
      </el-card>
    </template>
  </div>
</template>

<style scoped>
.log {
  font-family: var(--el-font-family, monospace);
  font-size: 12px;
  line-height: 1.7;
  white-space: pre-wrap;
  margin: 0;
}

/* 字段说明：小字灰色，跟在输入框下方 */
.tip {
  font-size: 12px;
  line-height: 1.6;
  color: var(--el-text-color-secondary);
  margin-top: 4px;
}

/* 校验失败提示：小字红色 */
.tip-error {
  font-size: 12px;
  line-height: 1.6;
  color: var(--el-color-danger);
  margin-top: 4px;
}

/* 品牌图行：缩略图 + 路径输入 + 选择/清除按钮 */
.brand-row {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
}

/* 主图缩略图：无图时显示灰色占位块，占位与图片同尺寸避免行高跳动 */
.brand-thumb {
  width: 40px;
  height: 40px;
  flex: none;
  border-radius: 6px;
  border: 1px solid var(--el-border-color);
  object-fit: cover;
  background: #fff;
}

.brand-thumb-empty {
  display: inline-block;
  background: var(--el-fill-color-light);
}

/* 悬空记录列表：一行一个，名字 + 路径 + 移除按钮 */
.broken-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 8px;
  font-size: 13px;
}

.broken-name {
  font-weight: 600;
  color: var(--el-color-danger);
}

.broken-dir {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

/* 选择已有子应用目录：每行一个目录卡片 */
.sub-row {
  border: 1px solid var(--el-border-color);
  border-radius: 6px;
  padding: 12px;
  margin-bottom: 10px;
}
.sub-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.sub-meta {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.sub-dir {
  margin: 6px 0 10px;
  font-size: 12px;
  color: var(--el-text-color-secondary);
  word-break: break-all;
}
.sub-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
</style>
