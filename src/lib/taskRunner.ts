import { computed, ref, type ComputedRef, type Ref } from 'vue';
import { api } from './tauri';
import type { TaskExit } from '../types';

/**
 * 全局任务管理器（单例）：跨页面持有任务输出。
 * 页面切换后构建继续跑，回到页面日志还在；全局「同时只有一个构建任务」的锁也在这里。
 */
const buffers = new Map<string, string[]>();
const exits = new Map<string, TaskExit>();
const running = new Set<string>();
const listeners = new Set<() => void>();
let subscribed = false;
let subscribing: Promise<void> | null = null;

function notify(): void {
  listeners.forEach((fn) => fn());
}

async function doSubscribe(): Promise<void> {
  await api.onTaskLine((l) => {
    const buf = buffers.get(l.taskId);
    if (buf) {
      buf.push(l.line);
      notify();
    }
  });
  await api.onTaskExit((e) => {
    running.delete(e.taskId);
    exits.set(e.taskId, e);
    notify();
  });
}

async function ensureSubscribed(): Promise<void> {
  if (subscribed) return;
  if (!subscribing) {
    // 只有两条监听都装好了才置 subscribed；失败时清掉缓存，让下次调用能重试。
    // （此前是进来就置 true，一旦首次注册失败，后续调用全部跳过注册——
    //   任务照跑但永远收不到 task-line/task-exit，界面卡在「进行中」且无日志。）
    subscribing = doSubscribe()
      .then(() => {
        subscribed = true;
      })
      .catch((e) => {
        subscribing = null;
        throw e;
      });
  }
  await subscribing;
}

export const taskRunner = {
  /** 是否有任意任务在跑（构建按钮的全局锁） */
  anyRunning: computed(() => running.size > 0),

  /** 当前运行中的任务 id 列表 */
  runningIds: computed(() => [...running]),

  async start(
    taskId: string,
    opts: { program: string; args: string[]; cwd: string; envs: Record<string, string> }
  ): Promise<void> {
    buffers.set(taskId, []);
    exits.delete(taskId);
    running.add(taskId);
    notify();
    try {
      await ensureSubscribed();
      await api.runTask({ taskId, ...opts });
    } catch (e) {
      // 任务没能启动（脚本不存在 / node 路径不对 / 权限不足等）：必须清理 running，
      // 否则界面永久停在「进行中」，全局任务锁再也放不开（之前就是这么卡死的）。
      // 同时把原因写进该任务的日志缓冲区，让用户直接看到而不是面对一个空白面板。
      running.delete(taskId);
      exits.set(taskId, { taskId, code: null, success: false });
      const buf = buffers.get(taskId) ?? [];
      buf.push(`[启动失败] ${e instanceof Error ? e.message : String(e)}`);
      buffers.set(taskId, buf);
      notify();
    }
  },

  async stop(taskId: string): Promise<void> {
    await api.stopTask(taskId);
  },

  /**
   * 强制把任务状态复位（不动进程，只清前端状态）。
   * 用于「进程其实早就结束、但退出事件没到」这类异常：否则 running 永远为真，
   * 按钮一直转圈、全局任务锁也放不开。
   */
  reset(taskId: string): void {
    running.delete(taskId);
    exits.set(taskId, { taskId, code: null, success: false });
    const buf = buffers.get(taskId) ?? [];
    buf.push('[已重置] 任务状态已被手动复位');
    buffers.set(taskId, buf);
    notify();
  },

  text(taskId: string): string {
    return (buffers.get(taskId) ?? []).join('\n');
  },

  isRunning(taskId: string): boolean {
    return running.has(taskId);
  },

  exitOf(taskId: string): TaskExit | undefined {
    return exits.get(taskId);
  },

  subscribe(cb: () => void): () => void {
    listeners.add(cb);
    return () => listeners.delete(cb);
  },
};

/** 把某个任务的输出变成响应式视图（tick 计数驱动 computed 重算） */
export function useTaskView(taskId: Ref<string>): {
  lines: ComputedRef<string[]>;
  running: ComputedRef<boolean>;
  exit: ComputedRef<TaskExit | undefined>;
} {
  const tick = ref(0);
  taskRunner.subscribe(() => {
    tick.value++;
  });
  return {
    lines: computed(() => {
      tick.value;
      return taskRunner.text(taskId.value).split('\n');
    }),
    running: computed(() => {
      tick.value;
      return taskRunner.isRunning(taskId.value);
    }),
    exit: computed(() => {
      tick.value;
      return taskRunner.exitOf(taskId.value);
    }),
  };
}
