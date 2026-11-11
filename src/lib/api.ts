import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export type TaskStage =
  | "queued"
  | "preparing"
  | "downloading"
  | "extracting"
  | "transcribing"
  | "done"
  | "error"
  | "canceled";

export interface Task {
  id: string;
  kind: "download" | "transcribe";
  title: string;
  url: string | null;
  stage: TaskStage;
  progress: number;
  speed: string;
  eta: string;
  message: string;
  filePath: string | null;
  srtPath: string | null;
  error: string | null;
  model: string | null;
  createdAt: number;
  log: string[];
}

export interface Settings {
  outputDir: string;
  quality: string;
  extractMp3: boolean;
  transcribe: boolean;
  language: string;
  model: string;
  openFolder: boolean;
  concurrency: number;
  useEmbeddedCookies: boolean;
  cookiesFile: string;
  autoDownloadModel: boolean;
  compute: "auto" | "cpu" | "gpu";
}

export interface ToolInfo {
  id: string;
  name: string;
  path: string | null;
  version: string | null;
  source: "app" | "system" | null;
}

export interface ModelInfo {
  id: string;
  label: string;
  file: string;
  size: number;
  installed: boolean;
  path: string;
}

export interface ToolchainStatus {
  ytDlp: ToolInfo;
  ffmpeg: ToolInfo;
  whisper: ToolInfo;
  whisperCuda: ToolInfo;
  whisperVulkan: ToolInfo;
  models: ModelInfo[];
  binDir: string;
  modelsDir: string;
  ready: boolean;
}

export interface GpuInfo {
  name: string;
  vendor: "nvidia" | "amd" | "intel" | "other" | string;
  vram: number;
}

export interface SystemInfo {
  cpu: string;
  cores: number;
  threads: number;
  ram: number;
  gpus: GpuInfo[];
  recommended: "cuda" | "vulkan" | "cpu" | string;
  cudaAvailable: boolean;
  vulkanAvailable: boolean;
}

export interface AssetProgress {
  kind: "tool" | "model";
  id: string;
  percent: number;
  downloaded: number;
  total: number;
  speed: number;
  message: string;
}

export interface CookieSummary {
  count: number;
  loggedIn: boolean;
  names: string[];
}

export interface MediaFile {
  path: string;
  name: string;
  size: number;
  hasMp3Sibling: boolean;
}

export interface Bounds {
  x: number;
  y: number;
  width: number;
  height: number;
}

const mockSettings: Settings = {
  outputDir: "D:\\youtube_videos",
  quality: "best",
  extractMp3: true,
  transcribe: true,
  language: "en",
  model: "base.en",
  openFolder: false,
  concurrency: 2,
  useEmbeddedCookies: true,
  cookiesFile: "",
  autoDownloadModel: true,
  compute: "auto",
};

const mockToolchain: ToolchainStatus = {
  ytDlp: {
    id: "yt-dlp",
    name: "yt-dlp",
    path: "C:\\Users\\objectMaker\\AppData\\Local\\Programs\\Python\\Python311\\Scripts\\yt-dlp.exe",
    version: "2026.10.31",
    source: "system",
  },
  ffmpeg: {
    id: "ffmpeg",
    name: "ffmpeg",
    path: "C:\\ffmpeg\\bin\\ffmpeg.exe",
    version: "ffmpeg version 8.1.2-full_build",
    source: "system",
  },
  whisper: { id: "whisper", name: "whisper.cpp (CPU)", path: null, version: null, source: null },
  whisperCuda: {
    id: "whisper-cuda",
    name: "whisper.cpp (NVIDIA CUDA)",
    path: null,
    version: null,
    source: null,
  },
  whisperVulkan: {
    id: "whisper-vulkan",
    name: "whisper.cpp (AMD/Intel Vulkan)",
    path: null,
    version: null,
    source: null,
  },
  models: [
    {
      id: "base.en",
      label: "Base (英文 · 推荐)",
      file: "ggml-base.en.bin",
      size: 147964211,
      installed: true,
      path: "",
    },
    {
      id: "large-v3-turbo",
      label: "Large v3 Turbo (多语言 · 最准)",
      file: "ggml-large-v3-turbo.bin",
      size: 1624555275,
      installed: false,
      path: "",
    },
  ],
  binDir: "%APPDATA%\\com.ybf.ytwhisper\\bin",
  modelsDir: "%APPDATA%\\com.ybf.ytwhisper\\models",
  ready: false,
};

const mockSystem: SystemInfo = {
  cpu: "Intel Core i7-9750H",
  cores: 6,
  threads: 12,
  ram: 17179869184,
  gpus: [{ name: "NVIDIA GeForce GTX 1660 Ti", vendor: "nvidia", vram: 6442450944 }],
  recommended: "cuda",
  cudaAvailable: true,
  vulkanAvailable: true,
};

const mockTasks: Task[] = [
  {
    id: "demo-1",
    kind: "download",
    title: "Rick Astley - Never Gonna Give You Up (Official Music Video)",
    url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
    stage: "downloading",
    progress: 42,
    speed: "12.4MiB/s",
    eta: "00:08",
    message: "下载中 60.0%",
    filePath: null,
    srtPath: null,
    error: null,
    model: "base.en",
    createdAt: Date.now(),
    log: ["已复用内置浏览器 YouTube 会话 Cookie", "[youtube] dQw4w9WgXcQ: Downloading webpage"],
  },
];

function mockCall(cmd: string, args?: Record<string, unknown>): unknown {
  switch (cmd) {
    case "get_settings":
      return mockSettings;
    case "save_settings":
      return (args as { settings: Settings }).settings;
    case "toolchain_status":
      return mockToolchain;
    case "system_info":
      return mockSystem;
    case "install_tool":
      return mockToolchain;
    case "cancel_download":
      return null;
    case "download_model":
      return mockToolchain;
    case "delete_model":
      return mockToolchain;
    case "list_tasks":
      return mockTasks;
    case "start_download":
      return mockTasks[0];
    case "start_transcribe":
      return [];
    case "scan_folder":
      return [];
    case "extract_youtube_cookies":
      return { count: 26, loggedIn: true, names: ["SID", "SAPISID", "LOGIN_INFO"] };
    case "browser_url":
      return null;
    default:
      return null;
  }
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri) return mockCall(cmd, args) as T;
  return invoke<T>(cmd, args);
}

async function subscribe<T>(
  event: string,
  cb: (payload: T) => void,
): Promise<UnlistenFn> {
  if (!isTauri) return () => {};
  return listen<T>(event, (e) => cb(e.payload));
}

export const api = {
  getSettings: () => call<Settings>("get_settings"),
  saveSettings: (settings: Settings) =>
    call<Settings>("save_settings", { settings }),
  toolchainStatus: () => call<ToolchainStatus>("toolchain_status"),
  systemInfo: () => call<SystemInfo>("system_info"),
  installTool: (id: string) => call<ToolchainStatus>("install_tool", { id }),
  cancelDownload: (kind: "tool" | "model", id: string) =>
    call<void>("cancel_download", { kind, id }),
  downloadModel: (id: string) => call<ToolchainStatus>("download_model", { id }),
  deleteModel: (id: string) => call<ToolchainStatus>("delete_model", { id }),
  listTasks: () => call<Task[]>("list_tasks"),
  cancelTask: (id: string) => call<void>("cancel_task", { id }),
  removeTask: (id: string) => call<void>("remove_task", { id }),
  startDownload: (url: string, options: { extractMp3?: boolean; transcribe?: boolean }) =>
    call<Task>("start_download", { url, options }),
  startTranscribe: (paths: string[], options: { model?: string; language?: string }) =>
    call<Task[]>("start_transcribe", { paths, options }),
  scanFolder: (folder: string) => call<MediaFile[]>("scan_folder", { folder }),
  openPath: (path: string) => call<void>("open_path", { path }),

  browserShow: (bounds: Bounds) => call<void>("browser_show", { bounds }),
  browserHide: () => call<void>("browser_hide"),
  browserSetBounds: (bounds: Bounds) => call<void>("browser_set_bounds", { bounds }),
  browserNavigate: (url: string) => call<void>("browser_navigate", { url }),
  browserBack: () => call<void>("browser_back"),
  browserForward: () => call<void>("browser_forward"),
  browserReload: () => call<void>("browser_reload"),
  browserUrl: () => call<string | null>("browser_url"),
  extractCookies: () => call<CookieSummary>("extract_youtube_cookies"),

  onTaskUpdated: (cb: (t: Task) => void) => subscribe<Task>("task-updated", cb),
  onTaskRemoved: (cb: (id: string) => void) => subscribe<string>("task-removed", cb),
  onAssetProgress: (cb: (p: AssetProgress) => void) =>
    subscribe<AssetProgress>("asset-progress", cb),
  onNavigated: (cb: (url: string) => void) => subscribe<string>("yt-navigated", cb),
};
