<script setup lang="ts">
import { reactive, ref, watch } from 'vue';
import { ElMessage } from 'element-plus';
import { saveSettings, settings } from '../lib/store';
import { api } from '../lib/tauri';
import { ENV_PRESETS } from '../types';

const draft = reactive({
  baseDir: settings.value.baseDir,
  nodePath: settings.value.nodePath,
  env: { ...settings.value.env } as Record<string, string>,
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
  saveSettings({ baseDir: draft.baseDir.trim(), nodePath: draft.nodePath.trim() || 'node', env });
  ElMessage.success('设置已保存');
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
