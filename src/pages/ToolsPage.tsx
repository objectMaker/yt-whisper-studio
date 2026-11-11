import { useEffect, useMemo, useRef, useState } from "react";
import {
  CheckCircle2,
  Cpu,
  Download,
  FolderOpen,
  HardDriveDownload,
  Languages,
  Loader2,
  Monitor,
  RefreshCw,
  Settings2,
  Sparkles,
  Trash2,
  Wrench,
  Zap,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import {
  api,
  isTauri,
  type AssetProgress,
  type ModelInfo,
  type Settings,
  type SystemInfo,
  type ToolchainStatus,
  type ToolInfo,
} from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Switch } from "@/components/ui/switch";
import { Select } from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { formatBytes, formatSpeed } from "@/lib/utils";

const LANG_OPTIONS = [
  { value: "auto", label: "自动检测" },
  { value: "en", label: "英语 English" },
  { value: "zh", label: "中文 简体" },
  { value: "ja", label: "日语 日本語" },
  { value: "ko", label: "韩语 한국어" },
  { value: "de", label: "德语" },
  { value: "fr", label: "法语" },
  { value: "es", label: "西班牙语" },
  { value: "ru", label: "俄语" },
];

const QUALITY_OPTIONS = [
  { value: "best", label: "最佳画质（自动合并）" },
  { value: "1080", label: "1080p" },
  { value: "720", label: "720p" },
  { value: "480", label: "480p" },
  { value: "360", label: "360p" },
];

const COMPUTE_OPTIONS = [
  { value: "auto", label: "自动（有可用加速版就用显卡）" },
  { value: "gpu", label: "优先显卡" },
  { value: "cpu", label: "只用 CPU" },
];

const TOOL_META: Record<string, { title: string; desc: string }> = {
  "yt-dlp": { title: "yt-dlp", desc: "视频解析与下载核心，可随时更新以适配 YouTube 改动" },
  ffmpeg: { title: "FFmpeg", desc: "音视频转码、MP3 提取、16k 单声道 WAV 生成" },
  whisper: { title: "whisper.cpp · CPU", desc: "通用构建，任何机器都能用，显存不足时自动走这里" },
  "whisper-cuda": {
    title: "whisper.cpp · CUDA",
    desc: "NVIDIA 显卡加速版，需要较新的 NVIDIA 驱动",
  },
  "whisper-vulkan": {
    title: "whisper.cpp · Vulkan",
    desc: "AMD / Intel 显卡加速版，走 Vulkan 驱动",
  },
};

function ToolCard({
  info,
  asset,
  busy,
  recommended,
  failed,
  onInstall,
  onCancel,
}: {
  info: ToolInfo;
  asset?: AssetProgress;
  busy: boolean;
  recommended?: boolean;
  failed?: string;
  onInstall: () => void;
  onCancel: () => void;
}) {
  const installed = !!info.path;
  const paused = asset?.message === "已暂停";
  const downloading = busy || (!!asset && asset.percent > 0 && asset.percent < 100 && !paused);
  return (
    <Card className="flex flex-col">
      <CardHeader className="pb-2">
        <div className="flex items-start justify-between">
          <div className="flex items-center gap-2">
            <div className="flex size-8 items-center justify-center rounded-lg bg-secondary/70">
              <Wrench className="size-4 text-primary" />
            </div>
            <div>
              <CardTitle className="flex items-center gap-1.5">
                {TOOL_META[info.id]?.title ?? info.name}
                {recommended && (
                  <Badge variant="success" className="ml-1">
                    <Zap className="size-2.5" /> 推荐
                  </Badge>
                )}
              </CardTitle>
              <CardDescription className="mt-0.5">
                {TOOL_META[info.id]?.desc}
              </CardDescription>
            </div>
          </div>
          {installed ? (
            <Badge variant="success">
              <CheckCircle2 className="size-3" /> 已安装
            </Badge>
          ) : (
            <Badge variant="warning">未安装</Badge>
          )}
        </div>
      </CardHeader>
      <CardContent className="mt-auto space-y-2 pt-2">
        <div className="rounded-md bg-secondary/30 px-2.5 py-1.5 font-mono text-[10px] text-muted-foreground">
          {failed ? (
            <span className="text-amber-400">{failed}</span>
          ) : info.path ? (
            `${info.version ?? "未知版本"} · ${info.source === "app" ? "应用私有" : "系统"}`
          ) : (
            "尚未检测到可执行文件"
          )}
        </div>
        {asset && asset.percent < 100 && !paused && !failed && (
          <div className="space-y-1">
            <Progress value={asset.percent} indeterminate={!asset.total} />
            <div className="flex justify-between text-[10px] text-muted-foreground">
              <span>{asset.percent ? `${asset.percent.toFixed(1)}%` : "下载中"}</span>
              <span>
                {formatBytes(asset.downloaded)}
                {asset.total ? ` / ${formatBytes(asset.total)}` : ""}
                {asset.speed ? ` · ${formatSpeed(asset.speed)}` : ""}
              </span>
            </div>
          </div>
        )}
        {downloading ? (
          <Button variant="secondary" size="sm" className="w-full" onClick={onCancel}>
            <Loader2 className="size-3.5 animate-spin" /> 暂停下载
          </Button>
        ) : (
          <Button variant="secondary" size="sm" className="w-full" onClick={onInstall}>
            <HardDriveDownload className="size-3.5" />
            {failed || paused ? "继续下载" : installed ? "重新安装 / 更新" : "一键安装"}
          </Button>
        )}
      </CardContent>
    </Card>
  );
}

export default function ToolsPage({
  settings,
  status,
  assets,
  onSaveSettings,
  onRefreshStatus,
  notify,
}: {
  settings: Settings | null;
  status: ToolchainStatus | null;
  assets: Record<string, AssetProgress>;
  onSaveSettings: (s: Settings) => void;
  onRefreshStatus: () => void;
  notify: (msg: string, kind?: "ok" | "err") => void;
}) {
  const [form, setForm] = useState<Settings | null>(settings);
  const [busy, setBusy] = useState<string | null>(null);
  const [sys, setSys] = useState<SystemInfo | null>(null);
  const [failed, setFailed] = useState<Record<string, string>>({});

  const setFail = (key: string, msg?: string) => {
    setFailed((prev) => {
      const next = { ...prev };
      if (msg) next[key] = msg;
      else delete next[key];
      return next;
    });
  };

  useEffect(() => setForm(settings), [settings]);

  // 实时保存时用 ref 取最新值，避免快速连续操作时闭包里的 form 过期
  const formRef = useRef<Settings | null>(form);
  useEffect(() => {
    formRef.current = form;
  }, [form]);

  useEffect(() => {
    api
      .systemInfo()
      .then(setSys)
      .catch((e) => notify(`硬件检测失败: ${e}`, "err"));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const tools = useMemo(
    () =>
      [status?.ytDlp, status?.ffmpeg, status?.whisper, status?.whisperCuda, status?.whisperVulkan].filter(
        Boolean,
      ) as ToolInfo[],
    [status],
  );

  const bestGpu = useMemo(() => {
    const list = (sys?.gpus ?? []).filter((g) => g.vendor === "nvidia" || g.vendor === "amd");
    return list.sort((a, b) => b.vram - a.vram)[0];
  }, [sys]);

  const modelFits = (m: ModelInfo) => {
    if (!sys) return true;
    if (form?.compute !== "cpu" && bestGpu && bestGpu.vram > 0) {
      return m.size * 1.15 + 500 * 1024 * 1024 < bestGpu.vram;
    }
    return m.size < sys.ram * 0.5;
  };

  const patch = (p: Partial<Settings>) => {
    const base = formRef.current ?? form;
    if (!base) return;
    const next = { ...base, ...p };
    formRef.current = next;
    setForm(next);
    // 即时生效：不再需要保存按钮
    void onSaveSettings(next);
  };

  const install = async (id: string) => {
    const key = `tool:${id}`;
    setFail(key);
    setBusy(id);
    try {
      await api.installTool(id);
      await onRefreshStatus();
      notify(`${TOOL_META[id]?.title ?? id} 安装完成`);
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("已暂停")) setFail(key, msg);
    } finally {
      setBusy(null);
    }
  };

  const cancel = (kind: "tool" | "model", id: string) => {
    api.cancelDownload(kind, id).catch(() => {});
  };

  const downloadModel = async (id: string) => {
    const key = `model:${id}`;
    setFail(key);
    setBusy(`model:${id}`);
    try {
      await api.downloadModel(id);
      await onRefreshStatus();
      notify(`模型 ${id} 下载完成`);
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("已暂停")) setFail(key, msg);
    } finally {
      setBusy(null);
    }
  };

  const removeModel = async (id: string) => {
    try {
      await api.deleteModel(id);
      await onRefreshStatus();
      notify(`已删除模型 ${id}`);
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const chooseDir = async () => {
    if (!isTauri) return notify("请在桌面应用中选择目录", "err");
    try {
      const dir = await open({ directory: true, multiple: false });
      if (typeof dir === "string") patch({ outputDir: dir });
    } catch (e) {
      notify(String(e), "err");
    }
  };

  return (
    <div className="h-full min-h-0 overflow-y-auto">
      <div className="mx-auto max-w-5xl space-y-6 p-5">
        <section className="space-y-3">
          <div className="flex items-center gap-2">
            <Monitor className="size-4 text-primary" />
            <h2 className="text-sm font-semibold">运行设备</h2>
          </div>
          <Card>
            <CardContent className="space-y-3 pt-5">
              <div className="grid gap-2 text-[11px] text-muted-foreground sm:grid-cols-2">
                <div className="flex items-center gap-2">
                  <Cpu className="size-3.5" />
                  <span className="truncate">{sys?.cpu || "正在检测处理器…"}</span>
                  {sys && sys.cores > 0 && (
                    <span className="font-mono text-[10px]">
                      {sys.cores}C/{sys.threads}T
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-2">
                  <Zap className="size-3.5" />
                  <span className="truncate">
                    {bestGpu
                      ? `${bestGpu.name}${bestGpu.vram ? ` · ${formatBytes(bestGpu.vram)} 显存` : ""}`
                      : sys
                        ? "未检测到独立显卡"
                        : "正在检测显卡…"}
                  </span>
                </div>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-[11px] text-muted-foreground">转录设备</span>
                <div className="flex overflow-hidden rounded-lg border border-border/70">
                  {COMPUTE_OPTIONS.map((o) => (
                    <button
                      key={o.value}
                      onClick={() => patch({ compute: o.value as Settings["compute"] })}
                      className={`px-3 py-1.5 text-[11px] transition ${
                        (form?.compute ?? "auto") === o.value
                          ? "bg-primary/15 text-primary"
                          : "text-muted-foreground hover:text-foreground"
                      }`}
                    >
                      {o.label}
                    </button>
                  ))}
                </div>
              </div>
              {sys && (
                <p className="text-[10px] text-muted-foreground">
                  检测结果：
                  {sys.recommended === "cuda"
                    ? "可用 NVIDIA CUDA 加速"
                    : sys.recommended === "vulkan"
                      ? "可用 Vulkan 加速（AMD / Intel）"
                      : "建议使用 CPU"}
                  。加速版未安装时会自动回退 CPU，不会失败。
                </p>
              )}
            </CardContent>
          </Card>
        </section>

        <section className="space-y-3">
          <div className="flex items-center justify-between">
            <div>
              <h2 className="text-sm font-semibold">工具链</h2>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                未检测到系统安装时，将下载到应用私有目录，不污染系统环境。
              </p>
            </div>
            <Button variant="ghost" size="sm" onClick={onRefreshStatus}>
              <RefreshCw className="size-3.5" /> 重新检测
            </Button>
          </div>
          <div className="grid grid-cols-3 gap-3">
            {tools.map((t) => (
              <ToolCard
                key={t.id}
                info={t}
                asset={assets[`tool:${t.id}`]}
                busy={busy === t.id}
                failed={failed[`tool:${t.id}`]}
                recommended={
                  (t.id === "whisper-cuda" && sys?.recommended === "cuda") ||
                  (t.id === "whisper-vulkan" && sys?.recommended === "vulkan")
                }
                onInstall={() => install(t.id)}
                onCancel={() => cancel("tool", t.id)}
              />
            ))}
          </div>
        </section>

        <section className="space-y-3">
          <div className="flex items-center gap-2">
            <Sparkles className="size-4 text-primary" />
            <h2 className="text-sm font-semibold">Whisper 模型</h2>
          </div>
          <div className="overflow-hidden rounded-xl border border-border/70">
            {(status?.models ?? []).map((m) => {
              const key = `model:${m.id}`;
              const progress = assets[key];
              const paused = progress?.message === "已暂停";
              const downloading =
                busy === key || (!!progress && progress.percent > 0 && progress.percent < 100 && !paused);
              return (
                <div
                  key={m.id}
                  className="flex items-center gap-3 border-b border-border/50 px-4 py-3 last:border-0"
                >
                  <Cpu className="size-4 shrink-0 text-muted-foreground" />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-xs font-medium">{m.label}</span>
                      <span className="font-mono text-[10px] text-muted-foreground">{m.id}</span>
                      {m.installed ? (
                        <Badge variant="success">已下载</Badge>
                      ) : (
                        <Badge variant="outline">未下载</Badge>
                      )}
                      <Badge variant={modelFits(m) ? "secondary" : "warning"}>
                        {modelFits(m) ? "本机可用" : "显存/内存偏低"}
                      </Badge>
                      {paused && <Badge variant="warning">已暂停</Badge>}
                    </div>
                    {failed[key] && (
                      <p className="mt-1 text-[10px] text-amber-400">{failed[key]}</p>
                    )}
                    {downloading && progress && (
                      <div className="mt-1.5 flex items-center gap-2">
                        <Progress value={progress.percent} className="flex-1" />
                        <span className="font-mono text-[10px] text-muted-foreground">
                          {progress.percent.toFixed(0)}%
                          {progress.total ? ` · ${formatBytes(progress.downloaded)}/${formatBytes(progress.total)}` : ""}
                          {progress.speed ? ` · ${formatSpeed(progress.speed)}` : ""}
                        </span>
                      </div>
                    )}
                  </div>
                  <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
                    {formatBytes(m.size)}
                  </span>
                  {m.installed ? (
                    <Button
                      variant="ghost"
                      size="icon-sm"
                      title="删除模型"
                      onClick={() => removeModel(m.id)}
                    >
                      <Trash2 className="size-3.5 text-muted-foreground" />
                    </Button>
                  ) : downloading ? (
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => cancel("model", m.id)}
                    >
                      <Loader2 className="size-3.5 animate-spin" /> 暂停
                    </Button>
                  ) : (
                    <Button variant="secondary" size="sm" onClick={() => downloadModel(m.id)}>
                      <Download className="size-3.5" />
                      {failed[key] || paused ? "继续下载" : "下载"}
                    </Button>
                  )}
                </div>
              );
            })}
            {!status && (
              <div className="px-4 py-6 text-center text-xs text-muted-foreground">加载中…</div>
            )}
          </div>
          <p className="text-[10px] text-muted-foreground">
            模型默认走 hf-mirror 国内镜像，工具走 gh-proxy 加速并自动回退官方源。下载可随时暂停，再次点击会从断点续传。
          </p>
        </section>

        <section className="space-y-3">
          <div className="flex items-center gap-2">
            <Settings2 className="size-4 text-primary" />
            <h2 className="text-sm font-semibold">默认设置</h2>
          </div>
          {form && (
            <Card>
              <CardContent className="space-y-4 pt-5">
                <div className="space-y-1.5">
                  <label className="text-[11px] text-muted-foreground">输出目录</label>
                  <div className="flex gap-2">
                    <div className="flex h-9 flex-1 items-center truncate rounded-lg border border-border bg-background/60 px-3 font-mono text-[11px] text-muted-foreground">
                      {form.outputDir}
                    </div>
                    <Button variant="secondary" size="icon" onClick={chooseDir} title="选择目录">
                      <FolderOpen className="size-4" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => api.openPath(form.outputDir)}
                      title="打开目录"
                    >
                      <FolderOpen className="size-4 text-muted-foreground" />
                    </Button>
                  </div>
                </div>

                <div className="grid grid-cols-2 gap-3">
                  <div className="space-y-1.5">
                    <label className="text-[11px] text-muted-foreground">默认画质</label>
                    <Select
                      value={form.quality}
                      onValueChange={(v) => patch({ quality: v })}
                      options={QUALITY_OPTIONS}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <label className="flex items-center gap-1 text-[11px] text-muted-foreground">
                      <Languages className="size-3" /> 默认字幕语言
                    </label>
                    <Select
                      value={form.language}
                      onValueChange={(v) => patch({ language: v })}
                      options={LANG_OPTIONS}
                    />
                  </div>
                </div>

                <div className="space-y-1.5">
                  <label className="text-[11px] text-muted-foreground">默认 Whisper 模型</label>
                  <Select
                    value={form.model}
                    onValueChange={(v) => patch({ model: v })}
                    options={(status?.models ?? []).map((m) => ({
                      value: m.id,
                      label: `${m.label}${m.installed ? "" : " · 未下载"}`,
                    }))}
                  />
                </div>

                <div className="space-y-2.5 rounded-lg border border-border/60 bg-secondary/20 p-3.5">
                  {[
                    {
                      key: "extractMp3" as const,
                      label: "下载后提取 MP3",
                      desc: "默认导出高质量 MP3 音频",
                    },
                    {
                      key: "transcribe" as const,
                      label: "下载后自动转录",
                      desc: "调用 whisper.cpp 生成同名 SRT 字幕",
                    },
                    {
                      key: "autoDownloadModel" as const,
                      label: "缺失模型时自动下载",
                      desc: "转录前自动补齐所需模型",
                    },
                    {
                      key: "useEmbeddedCookies" as const,
                      label: "复用内置浏览器 Cookie",
                      desc: "使用 WebView2 会话自动携带登录态",
                    },
                    {
                      key: "openFolder" as const,
                      label: "完成后打开文件夹",
                      desc: "任务完成后自动定位到输出目录",
                    },
                  ].map((item) => (
                    <label key={item.key} className="flex items-center justify-between gap-4">
                      <div>
                        <div className="text-xs">{item.label}</div>
                        <div className="text-[10px] text-muted-foreground">{item.desc}</div>
                      </div>
                      <Switch
                        checked={form[item.key]}
                        onCheckedChange={(v) => patch({ [item.key]: v } as Partial<Settings>)}
                      />
                    </label>
                  ))}
                </div>

                <div className="flex items-center justify-between pt-1">
                  <div className="flex gap-4 text-[10px] text-muted-foreground">
                    <button
                      className="hover:text-foreground"
                      onClick={() => status && api.openPath(status.binDir)}
                    >
                      应用工具目录
                    </button>
                    <button
                      className="hover:text-foreground"
                      onClick={() => status && api.openPath(status.modelsDir)}
                    >
                      模型目录
                    </button>
                  </div>
                  <span className="flex items-center gap-1.5 text-[10px] text-emerald-400/90">
                    <CheckCircle2 className="size-3" /> 修改即时生效
                  </span>
                </div>
              </CardContent>
            </Card>
          )}
        </section>
      </div>
    </div>
  );
}
