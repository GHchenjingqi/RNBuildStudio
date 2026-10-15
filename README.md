# RNBuild Studio — RNbase 可视化打包工作台

![RNBuild Studio banner](docs/banner.jpg)

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](./LICENSE)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24c8d8.svg)](https://tauri.app/)
[![Vue 3](https://img.shields.io/badge/Vue-3-42b883.svg)](https://vuejs.org/)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](./README.md)
[![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)](https://github.com/GHchenjingqi/RNBuildStudio/pulls)

> **English** — A desktop build studio for the [RNbase](https://github.com/GHchenjingqi/RNbase) hybrid app shell: fill in a form, it calls the existing RNbase build scripts, streams logs, and hands you the APK / HAP. No command line needed.
> 依赖的基座仓库：[**RNbase**](https://github.com/GHchenjingqi/RNbase)（RN + WebView + H5 混合架构基座）

Tauri 2 + Vue3 + Element Plus 的桌面打包工具，面向不懂代码的使用者：表单收集参数 → 调用 [RNbase](https://github.com/GHchenjingqi/RNbase) 现有打包脚本 → 日志流展示 → 产物页。

**核心原则：GUI 只做编排与参数收集，一切落盘动作走基座现有脚本**（`apply-app-meta.js` / `gradle.js` / `apk-hash.js`…），避免出现第二套打包逻辑漂移。

## 功能

- **打包向导**：选平台（**安卓 / 鸿蒙**；iOS 需 macOS 打包机，置灰）→ 选模式（子应用 / 远程 URL）→ 编辑子应用 `app.config.json`（保存后自动调 `apply-app-meta.js` 下发品牌）→ 选包类型（调试包 / 正式包）→ 构建 → 安卓自动 `apk:hash` 出 SHA-256 与升级清单字段；构建成功后可「安装到设备」
- 平台与命令映射（GUI 不发明新命令，与基座 `package.json` 的 npm scripts 一一对应）：
  | 平台 | 调试包 | 正式包 | 装机 |
  |---|---|---|---|
  | 安卓 | `scripts/gradle.js assembleDebug` | `scripts/gradle.js assembleRelease` | `scripts/gradle.js installDebug` |
  | 鸿蒙 | `scripts/hvigor.js assembleHap` | `scripts/hvigor.js assembleHap --release` | `scripts/hvigor.js assembleHap --install` |
- **新建应用**：三种类型——Vue 3 工程 / React 18 工程 / **仅配置（远程站点）**。调基座 `scripts/create-subapp.js` 在基座同级目录创建（复制模板 + 补齐 `scripts/` 三个对接脚本 + 写 `app.config.json` + 登记 `app-registry.json`；「仅配置」类型只写 `app.config.json` + `app-brand/`，无源码）。建好后可点「安装依赖」「构建并同步到基座」「设为当前应用」——仅配置类型只需最后一步
  - 子应用侧的 npm 命令以 `node <node_modules/npm/bin/npm-cli.js>` 方式执行：Windows 下 Rust 的 `Command` 不能直接跑 `npm.cmd`（不是 PE 可执行文件），且不经 `cmd` 可避免编码问题
- **参数可编辑**：打包向导里的应用名 / 包名 / 安装包文件名 / 版本 / 闪屏底色均可修改，保存即写回**所选应用**的 `app.config.json`；安装包文件名与远程地址带格式校验（前者会变成产物文件名，不能填网址）
- **打包目标强绑定**：保存配置走的是 `scripts/use-app.js`（而非直接 `apply-app-meta.js`），它会把所选应用写进注册表的 `activeApp`。`gradle.js` / `hvigor.js` 只认 `activeApp`，少了这一步会出现「选了 A 却打出 B」。进入构建前若选中项 ≠ activeApp 会被拦下并提示
- **悬空记录体检**：`list_apps` 会顺带回传每个应用的健康状态（目录 / `app.config.json` 是否还在）。删掉目录却在注册表留记录时，向导不再报错崩溃，而是把该项标灰并提示「从注册表移除」（`unregister_app` 命令，只改 `app-registry.json`，若它正好是 activeApp 则一并清空）
- **环境自检**：一键跑基座 `doctor.js`，三平台红绿灯 + 完整日志
- **设置**：基座路径、node 路径、工具链环境变量注入（`JAVA_HOME` / `ANDROID_HOME` / `DEVECO_HOME` / `HOS_HDC` 等——双击 exe 启动不继承 shell PATH）
- 全局任务锁：构建任务全局单并发（gradle/hvigor 不能并行）；页面切换后任务后台继续，日志不丢

## 目录

```
├─ src/                # Vue3 前端（打包向导 / 新建应用 / 环境自检 / 设置）
│  └─ lib/
│     ├─ tauri.ts      # invoke / listen 封装
│     ├─ taskRunner.ts # 全局任务管理器（跨页面日志流 + 任务锁）
│     └─ store.ts      # 设置持久化（localStorage）
└─ src-tauri/
   ├─ capabilities/
   │  └─ default.json  # ⚠️ 必需：core:default（含 core:event:allow-listen）
   └─ src/
      ├─ tasks.rs      # spawn node 脚本 + stdout/stderr 逐行事件 + kill
      └─ project.rs    # app-registry.json / app.config.json 读写 + 健康体检（只碰真源，不碰派生产物）
```

> **`src-tauri/capabilities/default.json` 不能删。** Tauri 2 的权限清单为空时，前端
> `listen()` 会被拒（自定义 `invoke` 命令不受影响）——现象是「任务确实在跑、产物也生成了，
> 但界面一行日志都没有、状态永远停在执行中」，排查起来非常绕。
> 可用 `src-tauri/gen/schemas/capabilities.json`（构建时生成）确认权限是否真的生效。

## 开发

```sh
npm install
npm run tauri:dev     # 需要 Rust 工具链（rustup）与 WebView2（Win10/11 自带）
```

dev server 固定 **1420** 端口（不用 Vite 默认的 5173，避免与开发机上其他项目撞端口
导致窗口加载到别人的页面）；`strictPort: true`，端口被占用时直接报错而不是静默换端口。

前端单独验证（不需要 Rust）：

```sh
npm run build         # vue-tsc 类型检查 + vite 构建
```

## 打 exe

```sh
npm run exe        # = tauri build --no-bundle：构建前端 + release 编译，只产 exe 不产安装器
```

产物：`src-tauri/target/release/rnbuild-studio.exe`。

要出安装包（NSIS/MSI）则把 `src-tauri/tauri.conf.json` 的 `bundle.active` 改为 `true`，再跑
`tauri build`。

### 只产 exe 不产安装器

- **必须走 `tauri build`（或 `npm run exe`），不要直接 `cargo build --release`。**
  `tauri build` 会自动带上 `--features custom-protocol`；缺这个 feature 时 `tauri` 以
  「dev 模式」编译，exe **不内嵌前端**，而是去连 `devUrl`——一旦那个端口跑着别的服务，
  exe 里就会显示出那个服务的页面（本项目最初就踩过：`devUrl` 用的是 Vite 默认端口
  5173，撞上了开发机上另一个项目）。
  该 feature 已在 `Cargo.toml` 的 `[features]` 中定义（`custom-protocol = ["tauri/custom-protocol"]`）。
- 第一次 `npx tauri icon <主图.png>` 生成 `src-tauri/icons/`（Windows 构建必须有 `icon.ico`）。
- 仍要手动跑 cargo 时，命令为：
  ```sh
  cd src-tauri && cargo build --release --features custom-protocol
  ```
  漏掉 `--features custom-protocol` 就会复现「exe 加载 5173」的问题。

## 与 RNbase 的关系

两个仓库互相独立：本仓是工作台**源码**，[RNbase](https://github.com/GHchenjingqi/RNbase) 是它操作的**基座**。
本仓不内置基座；把本仓构建出的 exe 拷进 RNbase 根目录运行即可（RNbase 侧 `*.exe` 已忽略，不会进基座 diff）。

## 零配置用法（推荐）

打包出的 `RNBuildStudio.exe` **直接放进 RNbase 根目录**运行即可：程序启动时自动探测
（`detect_base_dir` 命令会识别 exe 所在目录为 RNbase，或向上找兄弟目录里的 `RNbase`），
无需在设置页填写路径。非技术同学拿到 exe 后只要「丢进 RNbase 目录 → 双击」就能用。

> 例外：若 exe 放在别处（如桌面），启动页会提示未识别，到「设置」页点「自动探测」或手动粘贴路径即可。

## 边界与约定（与基座对齐）

- 只读写**真源文件**：`app-registry.json`、`<子应用>/app.config.json`；
  `version.json` / `res/` / `h5/` 等派生产物一律由基座脚本生成，GUI 不直写
- 签名口令（`QUX_UPLOAD_*`）不进项目目录，后续版本走系统凭据库（Electron safeStorage 等价物：Tauri stronghold / OS keychain）
- hdc 永远返回 0，鸿蒙装机状态必须解析输出而非看 exit code（构建脚本已按此口径，GUI 只透传日志）
- **读子进程输出必须容错，不能中途放弃管道**：早期版本用
  `BufReader::lines().map_while(Result::ok)` 转发日志，Windows 下 gradle / JDK 会吐
  GBK 字节，`lines()` 遇到非法 UTF-8 返回 Err → `map_while` 直接结束循环 →
  `ChildStdout` 被 drop → 管道读端关闭 → 基座侧 `gradle.js` 再写就是 EPIPE，
  打包被打成「失败」（构建其实还在跑）。现改为 `read_until(b'\n')` +
  `String::from_utf8_lossy`，读端活到子进程退出为止（见 `src-tauri/src/tasks.rs`
  的 `pump_lines`）。对应基座侧也已加 EPIPE 兜底，断管不再打断构建。
