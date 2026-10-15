// 复刻 src-tauri/src/project.rs 的 detect_base_dir + is_rnbase 逻辑做离线验证
const fs = require('fs');
const path = require('path');

function is_rnbase(dir) {
  return (
    fs.existsSync(path.join(dir, 'app-registry.json')) &&
    fs.existsSync(path.join(dir, 'scripts', 'apply-app-meta.js'))
  );
}

function detect_base_dir(exePath) {
  const parent = path.dirname(exePath);
  if (is_rnbase(parent)) return parent;
  // 向上遍历祖先，检查「祖先的父目录」下名为 RNbase 的兄弟
  let cur = parent;
  while (cur) {
    const grand = path.dirname(cur);
    if (grand && grand !== cur) {
      try {
        for (const name of fs.readdirSync(grand)) {
          if (name.toLowerCase() === 'rnbase') {
            const p = path.join(grand, name);
            if (is_rnbase(p)) return p;
          }
        }
      } catch {}
    }
    cur = grand;
  }
  if (is_rnbase(process.cwd())) return process.cwd();
  return '';
}

console.log('=== 场景 A：exe 丢进 RNbase 根目录 ===');
const a = detect_base_dir('D:\\codes\\RNbase\\RNBuildStudio.exe');
console.log('检测到 baseDir =', JSON.stringify(a));
console.log('绝对路径且为 RNbase？', a === 'D:\\codes\\RNbase' && is_rnbase(a));

console.log('\n=== 场景 B：开发态 exe 深埋 target/release ===');
const b = detect_base_dir('D:\\codes\\RNBuildStudio\\src-tauri\\target\\release\\RNBuildStudio.exe');
console.log('检测到 baseDir =', JSON.stringify(b));
console.log('自动找到兄弟 RNbase？', b === 'D:\\codes\\RNbase' && is_rnbase(b));
