<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { settings } from '../lib/store';
import { taskRunner, useTaskView } from '../lib/taskRunner';

/**
 * 新建子应用。
 *
 * 这里不复制任何文件——全部交给基座脚本 scripts/create-subapp.js：
 * 复制模板 → 补齐对接脚本 → 改写 package.json / index.html → 写 app.config.json →
 * 登记 app-registry.json。工作台只负责收集参数和转发日志，避免第二套建站逻辑漂移。
 */

const createTaskId = 'newapp';
const followTaskId = 'newapp-follow';
const { lines, running, exit } = useTaskView(ref(createTaskId));
const followLines = computed(() => taskRunner.text(followTaskId).split('\n'));
const followRunning = computed(() => taskRunner.isRunning(followTaskId));
const followExit = computed(() => taskRunner.exitOf(followTaskId));

const started = ref(false);
/** vue / react：完整工程（含源码）；config：仅配置应用（远程站点用，无源码） */
type Kind = 'vue' | 'react' | 'config';

const form = ref({
  name: '',
  framework: 'vue' as Kind,
  appName: '',
  applicationId: '',
  apkName: '',
  versionName: '1.0.0',
  versionCode: 1,
  splash: '#08080D',
  remoteUrl: '',
});

/** 仅配置应用：远程站点专用，只生成 app.config.json + app-brand/ */
const isConfigOnly = computed(() => form.value.framework === 'config');

/** 已成功创建的子应用目录名（成功后才开放后续动作） */
const created = ref<string | null>(null);

/** 目录名限制：字母开头 + 字母数字下划线连字符（会被写进注册表、拼进路径） */
const nameValid = computed(() => /^[a-zA-Z][a-zA-Z0-9_-]*$/.test(form.value.name));
/** 包名限制：反写域名，至少两段 */
const pkgValid = computed(
  () => /^[a-zA-Z][a-zA-Z0-9_.-]*$/.test(form.value.applicationId) &&
    form.value.applicationId.split('.').length >= 2,
);
const canCreate = computed(
  () =>
    nameValid.value &&
    pkgValid.value &&
    !!form.value.appName.trim() &&
    remoteValid.value &&
    !taskRunner.anyRunning.value
);

/** 仅配置应用必须带一个合法网址：它没有本地 H5 产物，地址为空就是白屏壳 */
const remoteValid = computed(
  () => !isConfigOnly.value || /^https?:\/\/\S+$/.test(form.value.remoteUrl.trim())
);

/** Windows 下 Rust 的 Command 不能直接跑 npm.cmd（不是 PE 可执行文件），
 *  改为用 node 执行 npm 的入口脚本：等价 `npm <args>`，且不经 cmd 避免编码问题。 */
const npmCli = computed(() => {
  const n = settings.value.nodePath.replace(/[\\/]+/g, '/');
  return `${n.slice(0, n.lastIndexOf('/'))}/node_modules/npm/bin/npm-cli.js`;
});

/** 子应用目录 = 基座的同级目录（两仓配合的硬约定） */
const subappDir = computed(() => {
  const b = settings.value.baseDir.replace(/[\\/]+/g, '/');
  return `${b.slice(0, b.lastIndexOf('/'))}/${created.value ?? form.value.name}`;
});

/** 名字空时自动推导包名/安装包名，减少用户填写量 */
function onNameBlur() {
  const base = form.value.name.replace(/_h5$/, '');
  const compact = base.replace(/[_-]/g, '');
  const camel = base
    .split(/[_-]/)
    .filter(Boolean)
    .map((s) => s.charAt(0).toUpperCase() + s.slice(1))
    .join('');
  if (!form.value.applicationId.trim() && compact) {
    form.value.applicationId = `com.qux.${compact.toLowerCase()}`;
  }
  if (!form.value.apkName.trim() && camel) {
    form.value.apkName = camel;
  }
}

async function create() {
  if (!settings.value.baseDir.trim()) {
    ElMessage.warning('请先在「设置」里指定基座目录');
    return;
  }
  started.value = true;
  created.value = null;
  const f = form.value;
  const args = [
    'scripts/create-subapp.js',
    '--name', f.name,
    '--app-name', f.appName.trim(),
    '--application-id', f.applicationId.trim(),
    '--apk-name', f.apkName.trim() || f.name,
    '--app-id', f.name,
    '--version-name', f.versionName,
    '--version-code', String(f.versionCode),
    '--splash', f.splash,
  ];
  if (isConfigOnly.value) {
    // 仅配置应用：不复制模板、不补脚本，只写 app.config.json + app-brand/
    args.push('--no-source', '--remote-url', f.remoteUrl.trim());
  } else {
    args.push('--framework', f.framework);
  }
  await taskRunner.start(createTaskId, {
    program: settings.value.nodePath,
    args,
    cwd: settings.value.baseDir,
    envs: settings.value.env,
  });
}

// 创建成功后把子应用名记下来，后续三个动作才有目标
watch(exit, (e) => {
  if (e?.success) created.value = form.value.name;
});

/** 复位：清前端任务状态 + 表单解锁。任务真卡住时用它自救（否则一直转圈、全局锁放不开）。 */
function resetAll() {
  taskRunner.reset(createTaskId);
  taskRunner.reset(followTaskId);
  started.value = false;
  created.value = null;
}

/** 后续动作：安装依赖 / 构建并同步 / 设为当前应用 */
async function runFollow(action: 'install' | 'build' | 'use') {
  if (!created.value) return;
  const args =
    action === 'use'
      ? // 仅配置应用没有 dist 产物，不能带 --sync（基座侧会去找不存在的 sync-to-base.js）
        ['scripts/use-app.js', created.value, ...(isConfigOnly.value ? [] : ['--sync'])]
      : [npmCli.value, action === 'install' ? 'install' : 'run', action === 'install' ? '' : 'build:base'].filter(
          (x) => x !== '',
        );
  await taskRunner.start(followTaskId, {
    program: settings.value.nodePath,
    args,
    cwd: action === 'use' ? settings.value.baseDir : subappDir.value,
    envs: settings.value.env,
  });
}
</script>

<template>
  <div style="padding: 20px; max-width: 820px">
    <h2 style="margin: 0 0 4px">新建应用</h2>
    <p style="color: var(--el-text-color-secondary); margin: 0 0 20px">
      在基座<strong>同级目录</strong>创建一个新应用，并自动登记到基座。选「Vue / React 工程」得到
      带源码的完整工程；选<strong>「仅配置」</strong>只建一份品牌与网址配置，用来把
      <strong>远程站点</strong>打包成 App（无需源码）。建好后回「打包向导」选它出包。
    </p>

    <el-form label-width="140px">
      <el-form-item label="类型">
        <el-radio-group v-model="form.framework" :disabled="started">
          <el-radio-button value="vue">Vue 3 工程</el-radio-button>
          <el-radio-button value="react">React 18 工程</el-radio-button>
          <el-radio-button value="config">仅配置（远程站点）</el-radio-button>
        </el-radio-group>
        <div class="tip">
          {{
            isConfigOnly
              ? '只生成 app.config.json 与 app-brand/，没有源码、不需要装依赖；适合给不同网址各打一个包'
              : '生成完整工程（含源码与构建脚本），需要 npm install 后构建'
          }}
        </div>
      </el-form-item>

      <el-form-item label="目录名" required>
        <el-input
          v-model="form.name"
          :placeholder="isConfigOnly ? '例如 xingce_app' : '例如 demo_h5（建议以 _h5 结尾）'"
          style="width: 320px"
          @blur="onNameBlur"
          :disabled="started"
        />
        <div v-if="form.name && !nameValid" class="err">
          只能用字母、数字、下划线、连字符，且必须以字母开头
        </div>
      </el-form-item>

      <el-form-item label="应用名" required>
        <el-input
          v-model="form.appName"
          placeholder="手机上显示的名字，如「古诗笺」"
          style="width: 320px"
          :disabled="started"
        />
      </el-form-item>

      <el-form-item label="应用包名" required>
        <el-input
          v-model="form.applicationId"
          placeholder="com.company.app"
          style="width: 320px"
          :disabled="started"
        />
        <div v-if="form.applicationId && !pkgValid" style="color: var(--el-color-danger); font-size: 12px">
          需为反写域名格式，至少两段（如 com.qux.demo）
        </div>
      </el-form-item>

      <el-form-item label="安装包文件名">
        <el-input v-model="form.apkName" placeholder="Demo" style="width: 320px" :disabled="started" />
      </el-form-item>

      <el-form-item label="版本">
        <el-input v-model="form.versionName" placeholder="1.0.0" style="width: 160px" :disabled="started" />
        <span style="margin: 0 12px">内部版本号</span>
        <el-input-number v-model="form.versionCode" :min="1" :disabled="started" />
      </el-form-item>

      <el-form-item v-if="isConfigOnly" label="远程网址" required>
        <el-input
          v-model="form.remoteUrl"
          placeholder="https://…（应用启动后直接加载这个网址）"
          style="width: 420px"
          :disabled="started"
        />
        <div v-if="form.remoteUrl && !remoteValid" class="err">
          需为 http:// 或 https:// 开头的完整网址
        </div>
      </el-form-item>

      <el-form-item label="闪屏底色">
        <el-color-picker v-model="form.splash" :disabled="started" />
        <span style="margin-left: 12px">{{ form.splash }}</span>
      </el-form-item>

      <el-form-item>
        <el-button
          type="primary"
          :loading="running"
          :disabled="!canCreate"
          @click="create"
        >
          {{ running ? '创建中…' : isConfigOnly ? '创建配置应用' : '创建应用' }}
        </el-button>
        <el-button v-if="running" type="danger" plain @click="taskRunner.stop(createTaskId)">
          停止
        </el-button>
        <el-button v-if="started" @click="resetAll">重新填写</el-button>
        <el-tag v-if="exit" :type="exit.success ? 'success' : 'danger'" style="margin-left: 12px">
          {{ exit.success ? '创建成功' : `创建失败（exit ${exit.code ?? '已终止'}）` }}
        </el-tag>
      </el-form-item>
    </el-form>

    <el-card v-if="started" shadow="never">
      <template #header>创建日志</template>
      <pre class="log">{{ lines.join('\n').trim() || (running ? '正在执行…' : '（暂无输出）') }}</pre>
    </el-card>

    <el-card v-if="created" shadow="never" style="margin-top: 16px">
      <template #header>
        后续动作（目标：{{ subappDir }}）
      </template>
      <el-space>
        <el-button
          v-if="!isConfigOnly"
          :loading="followRunning"
          :disabled="taskRunner.anyRunning.value"
          @click="runFollow('install')"
        >
          安装依赖
        </el-button>
        <el-button
          v-if="!isConfigOnly"
          :loading="followRunning"
          :disabled="taskRunner.anyRunning.value"
          @click="runFollow('build')"
        >
          构建并同步到基座
        </el-button>
        <el-button
          :loading="followRunning"
          :disabled="taskRunner.anyRunning.value"
          @click="runFollow('use')"
        >
          设为当前应用
        </el-button>
        <el-tag v-if="followExit" :type="followExit.success ? 'success' : 'danger'">
          {{ followExit.success ? '执行成功' : `失败（exit ${followExit.code ?? '已终止'}）` }}
        </el-tag>
      </el-space>
      <p v-if="isConfigOnly" style="color: var(--el-text-color-secondary); font-size: 12px; margin: 12px 0 0">
        仅配置应用没有源码与产物，直接点「设为当前应用」即可（会自动下发品牌），之后回「打包向导」选
        「远程站点」出包。
      </p>
      <p v-if="!isConfigOnly" style="color: var(--el-text-color-secondary); font-size: 12px; margin: 12px 0 0">
        按顺序点：安装依赖 → 构建并同步到基座 → 设为当前应用。之后回到「打包向导」出包。
      </p>
      <pre v-if="followLines.length > 1" class="log" style="margin-top: 12px">{{ followLines.join('\n') }}</pre>
    </el-card>
  </div>
</template>

<style scoped>
.log {
  max-height: 300px;
  overflow: auto;
  margin: 0;
  padding: 10px;
  background: var(--el-fill-color-light);
  border-radius: 4px;
  font-size: 12px;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-all;
}

/* 字段说明：小字灰色 */
.tip {
  font-size: 12px;
  line-height: 1.6;
  color: var(--el-text-color-secondary);
  margin-top: 4px;
}

/* 校验失败提示：小字红色 */
.err {
  font-size: 12px;
  line-height: 1.6;
  color: var(--el-color-danger);
  margin-top: 4px;
}
</style>
