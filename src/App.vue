<script setup lang="ts">
import { onMounted, ref } from 'vue';
import EnvCheck from './pages/EnvCheck.vue';
import BuildWizard from './pages/BuildWizard.vue';
import NewApp from './pages/NewApp.vue';
import Settings from './pages/Settings.vue';
import { api } from './lib/tauri';
import { saveSettings, settings } from './lib/store';

const page = ref<'env' | 'build' | 'new' | 'settings'>('build');
const pages = {
  env: { title: '环境自检', comp: EnvCheck },
  build: { title: '打包向导', comp: BuildWizard },
  new: { title: '新建应用', comp: NewApp },
  settings: { title: '设置', comp: Settings },
} as const;

// 启动即自动探测基座目录：用户把 exe 丢进 RNbase 根目录时，全程零配置。
onMounted(async () => {
  if (!settings.value.baseDir.trim()) {
    try {
      const dir = await api.detectBaseDir();
      if (dir) saveSettings({ ...settings.value, baseDir: dir });
    } catch {
      /* 探测失败不阻塞启动，用户可在设置页手动填 */
    }
  }
});
</script>

<template>
  <el-container style="height: 100vh">
    <el-aside width="180px" style="border-right: 1px solid var(--el-border-color)">
      <div style="padding: 16px; font-weight: 600">RNBuild Studio</div>
      <el-menu :default-active="page" @select="(k: string) => (page = k as typeof page)">
        <el-menu-item index="build">打包向导</el-menu-item>
        <el-menu-item index="new">新建应用</el-menu-item>
        <el-menu-item index="env">环境自检</el-menu-item>
        <el-menu-item index="settings">设置</el-menu-item>
      </el-menu>
    </el-aside>
    <el-main style="padding: 0">
      <component :is="pages[page].comp" />
    </el-main>
  </el-container>
</template>
