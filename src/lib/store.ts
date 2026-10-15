import { ref } from 'vue';
import { DEFAULT_SETTINGS, type Settings } from '../types';

const KEY = 'rnbuild-studio.settings';

export const settings = ref<Settings>(load());

function load(): Settings {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      return { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
    }
  } catch {
    /* 损坏的存储按默认值重来 */
  }
  return { ...DEFAULT_SETTINGS };
}

export function saveSettings(next: Settings): void {
  settings.value = { ...next };
  localStorage.setItem(KEY, JSON.stringify(next));
}
