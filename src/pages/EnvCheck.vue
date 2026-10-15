<script setup lang="ts">
import { computed, ref } from 'vue';
import { settings } from '../lib/store';
import { taskRunner, useTaskView } from '../lib/taskRunner';

const taskId = ref('doctor');
const { lines, running, exit } = useTaskView(taskId);
const started = ref(false);

/** doctor 输出标记：[OK] / [WARN] / [INFO] / [FAIL]，用于给日志行上色 */
function lineClass(line: string): string {
  if (line.includes('[OK]')) return 'line-ok';
  if (line.includes('[WARN]') || line.includes('[FAIL]')) return 'line-warn';
  if (line.includes('[INFO]')) return 'line-info';
  return '';
}

/** 平台链路状态：从「平台链路」一节的行里解析三平台红绿灯 */
const platformStatus = computed(() => {
  const status: Record<'android' | 'harmony' | 'ios', 'ok' | 'warn' | 'info' | 'unknown'> = {
    android: 'unknown',
    harmony: 'unknown',
    ios: 'unknown',
  };
  for (const line of lines.value) {
    if (line.includes('安卓：') && line.includes('[OK]')) status.android = 'ok';
    if (line.includes('鸿蒙链路未就绪')) status.harmony = 'warn';
    else if (line.includes('鸿蒙：') && line.includes('[OK]')) status.harmony = 'ok';
    if (line.includes('iOS：')) status.ios = line.includes('[FAIL]') ? 'warn' : 'info';
  }
  return status;
});

async function runDoctor() {
  started.value = true;
  await taskRunner.start(taskId.value, {
    program: settings.value.nodePath,
    args: ['scripts/doctor.js'],
    cwd: settings.value.baseDir,
    envs: settings.value.env,
  });
}
</script>

<template>
  <div style="padding: 20px">
    <el-alert
      type="info"
      :closable="false"
      show-icon
      title="打包前先跑环境自检（npm run doctor）：只读检查依赖、活动子应用、h5 产物、api.js 一致性与三平台工具链，缺什么会在这里列出来"
      style="margin-bottom: 16px"
    />
    <el-space>
      <el-button type="primary" :loading="running" :disabled="taskRunner.anyRunning.value" @click="runDoctor">
        {{ running ? '检查中…' : '运行环境自检' }}
      </el-button>
      <el-tag v-if="exit" :type="exit.success ? 'success' : 'danger'">
        {{ exit.success ? '自检完成（无 FAIL）' : `存在 FAIL（exit ${exit.code ?? '?'})` }}
      </el-tag>
    </el-space>

    <el-row v-if="started" :gutter="12" style="margin-top: 16px">
      <el-col :span="8">
        <el-card shadow="never">
          <div>安卓</div>
          <el-tag :type="platformStatus.android === 'ok' ? 'success' : 'info'" style="margin-top: 8px">
            {{ platformStatus.android === 'ok' ? '链路就绪' : '以日志为准' }}
          </el-tag>
        </el-card>
      </el-col>
      <el-col :span="8">
        <el-card shadow="never">
          <div>鸿蒙</div>
          <el-tag :type="platformStatus.harmony === 'ok' ? 'success' : 'warning'" style="margin-top: 8px">
            {{ platformStatus.harmony === 'ok' ? '前置通过' : '未就绪（不影响安卓）' }}
          </el-tag>
        </el-card>
      </el-col>
      <el-col :span="8">
        <el-card shadow="never">
          <div>iOS</div>
          <el-tag type="info" style="margin-top: 8px">出包仅 macOS，以日志为准</el-tag>
        </el-card>
      </el-col>
    </el-row>

    <el-card v-if="started" shadow="never" style="margin-top: 16px">
      <template #header>自检日志</template>
      <pre class="log" style="max-height: 380px; overflow: auto; margin: 0"><span
        v-for="(l, i) in lines"
        :key="i"
        :class="lineClass(l)"
      >{{ l }}
</span></pre>
    </el-card>
  </div>
</template>

<style scoped>
.log {
  font-family: var(--el-font-family, monospace);
  font-size: 12px;
  line-height: 1.7;
  white-space: pre-wrap;
}
.line-ok {
  color: var(--el-color-success);
}
.line-warn {
  color: var(--el-color-danger);
}
.line-info {
  color: var(--el-color-info);
}
</style>
