# YT Whisper Studio

把「YouTube 下载 → 提取音频 → whisper.cpp 生成字幕」这条链路做成一个 Windows 桌面应用。
内置浏览器完成登录，登录态自动复用给 yt-dlp；全程本地转写，不上传任何音视频。

## 功能

- **内置 YouTube 浏览器**：基于 WebView2 的原生子 WebView，登录后自动读取 Cookie 并生成临时 Cookie 文件，无需手动导出 cookies.txt。
- **登录态下载**：yt-dlp 始终带 Cookie 运行，避免匿名请求被风控；自动处理 n-signature 挑战（deno / node）。
- **一键出字幕**：下载完成后自动提取 MP3、生成 16k 单声道 WAV，调用本地 whisper.cpp 输出 SRT。
- **多加速后端**：CPU / NVIDIA CUDA / AMD·Intel Vulkan 三套 whisper.cpp，自动检测 CPU、显卡与显存并推荐；加速版不可用时自动回退 CPU。
- **模型管理**：tiny / base / small / medium / large-v3-turbo，含英文专用版与多语言版，以及 q5_0 量化版；按本机显存/内存标注可用性。
- **工具链托管**：yt-dlp、ffmpeg、whisper.cpp 一键安装到应用私有目录，不污染系统环境，可单独更新或重装。
- **国内下载加速**：GitHub 资源走 gh-proxy / ghfast 镜像，Hugging Face 模型走 hf-mirror，失败自动切换源。
- **断点续传与暂停**：下载中和已暂停一目了然，继续时从断点续传；解压安装前会校验文件完整性。
- **系统代理复用**：自动读取 Windows 系统代理设置，下载与 yt-dlp 和浏览器走同一条线路。
- **任务面板**：实时进度、速度、ETA 与完整日志，可取消、可打开产物目录。

## 界面

- 浏览器页：内嵌 YouTube，地址栏可直接导航
- 任务页：下载与转写进度、日志
- 工具页：运行设备检测、转录后端选择、工具链与模型管理

## 技术栈

| 层 | 选型 |
| --- | --- |
| 桌面框架 | Tauri v2（`unstable` 多 WebView） |
| 前端 | React 19 + TypeScript + Vite + Tailwind CSS v4 |
| 组件 | shadcn/ui 风格组件（Radix UI） |
| 后端 | Rust + tokio，调用 yt-dlp / ffmpeg / whisper-cli |
| 浏览器 | WebView2（`webview2-com`），复用系统登录态 |
| 打包 | NSIS（Windows x64） |

## 环境要求

- Windows 10/11（WebView2 运行时系统自带）
- Node.js 20+
- Rust stable（MSVC 工具链）
- Visual Studio Build Tools（C++ 生成工具）

## 开发

```bash
npm install
npm run tauri dev
```

前端单独调试：`npm run dev`（Vite，默认 http://localhost:1420）

## 构建安装包

```bash
npm run tauri build
```

产物：`src-tauri/target/release/bundle/nsis/YT-Whisper Studio_0.1.0_x64-setup.exe`

> `src-tauri/resources/whisper-vulkan.zip` 是内置的 Vulkan 版 whisper.cpp 组件，构建时必须存在。

## 目录结构

```
src/                      前端（页面、组件、API 封装）
src-tauri/src/
  browser.rs              内嵌 YouTube WebView 与 Cookie 提取
  cookies.rs              临时 Cookie 文件
  tasks.rs                下载 / 转写任务编排
  tools.rs                工具链与模型安装、断点续传
  hardware.rs             CPU / 显卡检测与后端推荐
  net.rs                  镜像候选与系统代理
  settings.rs             配置持久化
  lib.rs                  Tauri 命令注册
src-tauri/resources/      内置 Vulkan whisper 组件
```

## 数据位置

工具、模型、任务产物均保存在应用数据目录（`com.ybf.ytwhisper`），卸载应用不会影响你输出到自定义目录的文件。
下载中的临时文件为 `*.download`，重启应用后可继续。

## 注意事项

- 本项目仅用于个人备份与学习，请遵守 YouTube 服务条款与当地版权法规。
- Cookie 只在本地使用，不会上传；应用不内置任何账号凭据。
- 首次使用需在「工具」页安装工具链并下载模型，之后可完全离线转写。

## License

暂未选择开源协议。如需用于分发，请先补充 LICENSE。
