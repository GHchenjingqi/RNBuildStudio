<script setup lang="ts">
import { computed, reactive, ref, watch } from 'vue';
import { ElMessage, ElMessageBox } from 'element-plus';
import { saveSettings, settings } from '../lib/store';
import { api } from '../lib/tauri';
import { ENV_PRESETS } from '../types';

const draft = reactive({
  baseDir: settings.value.baseDir,
  nodePath: settings.value.nodePath,
  env: { ...settings.value.env } as Record<string, string>,
  signing: { ...settings.value.signing },
});

const valid = ref<boolean | null>(null);
async function recheck(path: string) {
  if (!path || !path.trim()) {
    valid.value = null;
    return;
  }
  valid.value = await api.validateBaseDir(path.trim());
}
watch(() => draft.baseDir, (v) => void recheck(v), { immediate: true });

async function detect() {
  const dir = await api.detectBaseDir();
  if (dir) {
    draft.baseDir = dir;
    ElMessage.success('已自动探测到 RNbase：' + dir);
  } else {
    ElMessage.warning('未自动找到 RNbase，请手动填写路径（或把 exe 放到 RNbase 根目录）');
  }
}

function save() {
  const env: Record<string, string> = {};
  for (const [k, v] of Object.entries(draft.env)) {
    if (v && v.trim()) env[k] = v.trim();
  }
  saveSettings({
    baseDir: draft.baseDir.trim(),
    nodePath: draft.nodePath.trim() || 'node',
    env,
    signing: { ...draft.signing },
  });
  ElMessage.success('设置已保存');
}

// 安卓正式包签名：缺密钥库时一键生成；已存在时一键校验口令/别名是否正确
const keystoreBusy = ref(false);
const keystoreMsg = ref('');
const keystorePath = computed(() => {
  const d = draft.baseDir.trim();
  const n = (draft.signing.keystoreName || '').trim();
  return n ? `${d}/platforms/android/app/${n}` : '';
});
async function genOrVerifyKeystore() {
  if (!keystorePath.value) {
    ElMessage.warning('请先填写密钥库文件名');
    return;
  }
  keystoreBusy.value = true;
  keystoreMsg.value = '';
  const opts = {
    androidAppDir: `${draft.baseDir.trim()}/platforms/android/app`,
    keystoreName: draft.signing.keystoreName.trim() || 'QUX-release.keystore',
    alias: draft.signing.alias.trim() || 'qux',
    storePassword: draft.signing.storePassword,
    keyPassword: draft.signing.keyPassword,
    envs: draft.env,
  };
  try {
    // 文件已存在 → 校验口令/别名；不存在 → 生成
    if (await api.verifyReleaseKeystore(opts).then((v) => v.exists).catch(() => false)) {
      const v = await api.verifyReleaseKeystore(opts);
      keystoreMsg.value = v.message;
      if (v.ok) {
        ElMessage.success(v.message);
      } else {
        ElMessage.error(v.message);
      }
    } else {
      const r = await api.ensureReleaseKeystore(opts);
      keystoreMsg.value = r.existed ? '密钥库已存在，无需重新生成' : `已生成密钥库：${r.path}`;
      ElMessage.success(keystoreMsg.value);
    }
  } catch (e) {
    keystoreMsg.value = String(e);
    ElMessage.error(String(e));
  } finally {
    keystoreBusy.value = false;
  }
}

/** 强制重新生成：删掉旧密钥库，用当前设置（默认 android/android/qux）重建。 */
async function forceRegenKeystore() {
  if (!keystorePath.value) {
    ElMessage.warning('请先填写密钥库文件名');
    return;
  }
  try {
    await ElMessageBox.confirm(
      '将删除现有密钥库，并用当前设置重新生成一个新密钥库。' +
        '用旧密钥库签过的 APK 将无法覆盖安装新包（需先卸载旧 APK）。' +
        '若这是生产正式签名，切勿继续（会作废正式签名且无法恢复）！',
      '强制重新生成密钥库',
      { confirmButtonText: '删除并重新生成', cancelButtonText: '取消', type: 'warning' },
    );
  } catch {
    return; // 用户取消
  }
  keystoreBusy.value = true;
  keystoreMsg.value = '';
  try {
    const r = await api.ensureReleaseKeystore({
      androidAppDir: `${draft.baseDir.trim()}/platforms/android/app`,
      keystoreName: draft.signing.keystoreName.trim() || 'QUX-release.keystore',
      alias: draft.signing.alias.trim() || 'qux',
      storePassword: draft.signing.storePassword,
      keyPassword: draft.signing.keyPassword,
      envs: draft.env,
      force: true,
    });
    keystoreMsg.value = `已删除旧密钥库并重新生成：${r.path}`;
    ElMessage.success(keystoreMsg.value);
  } catch (e) {
    keystoreMsg.value = String(e);
    ElMessage.error(String(e));
  } finally {
    keystoreBusy.value = false;
  }
}
</script>

<template>
  <div style="padding: 20px; max-width: 720px">
    <el-form label-width="180px">
      <el-form-item label="RNbase 基座路径">
        <el-input v-model="draft.baseDir" placeholder="D:/codes/RNbase">
          <template #append>
            <el-button @click="detect">自动探测</el-button>
          </template>
        </el-input>
        <div class="tip">
          <span v-if="valid === true" style="color: #67c23a">✓ 已识别为 RNbase 基座</span>
          <span v-else-if="valid === false" style="color: #f56c6c"
            >✗ 该目录不是 RNbase（需含 app-registry.json 与 scripts/apply-app-meta.js）</span
          >
          <span v-else>打包脚本都在该目录 scripts/ 下运行；子应用按其 app-registry.json 定位</span>
        </div>
        <div class="tip">零配置用法：把本 exe 直接放进 RNbase 根目录，启动会自动识别，无需填写</div>
      </el-form-item>
      <el-form-item label="node 可执行文件">
        <el-input v-model="draft.nodePath" placeholder="node 或绝对路径" />
        <div class="tip">双击 exe 启动时 PATH 里可能没有 node，可填绝对路径</div>
      </el-form-item>

      <el-divider content-position="left">工具链环境变量（留空 = 不注入）</el-divider>
      <el-form-item v-for="item in ENV_PRESETS" :key="item.key" :label="item.key">
        <el-input v-model="draft.env[item.key]" placeholder="可选" />
        <div class="tip">{{ item.desc }}</div>
      </el-form-item>

      <el-divider content-position="left">安卓正式包签名</el-divider>
      <el-alert
        type="warning"
        :closable="false"
        show-icon
        style="margin-bottom: 16px"
        title="生产发布务必替换为自有密钥库并妥善备份：丢失该文件将无法再为同一应用发布更新"
      />
      <el-form-item label="密钥库文件名">
        <el-input v-model="draft.signing.keystoreName" placeholder="QUX-release.keystore" />
        <div class="tip">
          落在 &lt;基座&gt;/platforms/android/app/ 下；被 *.keystore 规则忽略、不随仓库同步。
          构建正式包时若此文件不存在，打包器会用 keytool 自动生成（需本机 JDK / JAVA_HOME）
        </div>
      </el-form-item>
      <el-form-item label="密钥别名">
        <el-input v-model="draft.signing.alias" placeholder="qux" />
      </el-form-item>
      <el-form-item label="密钥库口令">
        <el-input
          v-model="draft.signing.storePassword"
          type="password"
          show-password
          placeholder="android"
        />
      </el-form-item>
      <el-form-item label="密钥口令">
        <el-input
          v-model="draft.signing.keyPassword"
          type="password"
          show-password
          placeholder="android"
        />
      </el-form-item>
      <el-form-item label="密钥库路径">
        <el-input :model-value="keystorePath" readonly />
        <div class="tip">解析后的绝对路径；点下方按钮可立即生成（不存在时）或校验口令/别名（已存在时）</div>
      </el-form-item>
      <el-form-item>
        <el-button :loading="keystoreBusy" @click="genOrVerifyKeystore">生成 / 校验密钥库</el-button>
        <el-button :loading="keystoreBusy" type="warning" plain @click="forceRegenKeystore">
          强制重新生成
        </el-button>
        <span v-if="keystoreMsg" class="tip" :style="{ color: keystoreMsg.includes('正确') || keystoreMsg.includes('已生成') || keystoreMsg.includes('重新生成') ? '#67c23a' : '#f56c6c' }">{{ keystoreMsg }}</span>
      </el-form-item>

      <el-form-item>
        <el-button type="primary" @click="save">保存设置</el-button>
      </el-form-item>
    </el-form>
  </div>
</template>

<style scoped>
.tip {
  font-size: 12px;
  color: var(--el-text-color-secondary);
  line-height: 1.5;
  margin-top: 4px;
}
</style>
