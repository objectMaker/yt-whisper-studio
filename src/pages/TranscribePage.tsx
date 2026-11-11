import { useCallback, useEffect, useRef, useState } from "react";
import {
  AudioLines,
  Check,
  FileAudio,
  FolderSearch,
  Plus,
  Sparkles,
  Trash2,
  UploadCloud,
  X,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api, isTauri, type MediaFile, type Settings, type Task, type ToolchainStatus } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Select } from "@/components/ui/select";
import { Progress } from "@/components/ui/progress";
import { formatBytes } from "@/lib/utils";
import { isActive, stageMeta } from "@/lib/stage";

const LANG_OPTIONS = [
  { value: "auto", label: "自动检测" },
  { value: "en", label: "英语 English" },
  { value: "zh", label: "中文 简体" },
  { value: "ja", label: "日语 日本語" },
  { value: "ko", label: "韩语 한국어" },
  { value: "de", label: "德语 Deutsch" },
  { value: "fr", label: "法语 Français" },
  { value: "es", label: "西班牙语 Español" },
  { value: "ru", label: "俄语 Русский" },
];

export default function TranscribePage({
  tasks,
  status,
  settings,
  notify,
}: {
  tasks: Task[];
  status: ToolchainStatus | null;
  settings: Settings | null;
  notify: (msg: string, kind?: "ok" | "err") => void;
}) {
  const [files, setFiles] = useState<MediaFile[]>([]);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [model, setModel] = useState(settings?.model ?? "base.en");
  const [language, setLanguage] = useState(settings?.language ?? "en");
  const [dragging, setDragging] = useState(false);
  const unlistenRef = useRef<(() => void) | null>(null);

  const addPaths = useCallback((paths: string[]) => {
    setFiles((prev) => {
      const known = new Set(prev.map((f) => f.path));
      const next = [...prev];
      const added: string[] = [];
      for (const p of paths) {
        if (known.has(p)) continue;
        const name = p.split(/[\\/]/).pop() || p;
        next.push({ path: p, name, size: 0, hasMp3Sibling: false });
        added.push(p);
      }
      if (added.length) {
        setSelected((s) => new Set([...s, ...added]));
      }
      return next;
    });
  }, []);

  useEffect(() => {
    if (!isTauri) return;
    let disposed = false;
    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "over") setDragging(true);
        else if (event.payload.type === "drop") {
          setDragging(false);
          addPaths(event.payload.paths);
        } else setDragging(false);
      })
      .then((un) => {
        if (disposed) un();
        else unlistenRef.current = un;
      });
    return () => {
      disposed = true;
      unlistenRef.current?.();
    };
  }, [addPaths]);

  useEffect(() => {
    if (settings) {
      setModel(settings.model);
      setLanguage(settings.language);
    }
  }, [settings]);

  const pickFiles = async () => {
    if (!isTauri) return notify("请在桌面应用中选择文件", "err");
    try {
      const picked = await open({
        multiple: true,
        filters: [
          {
            name: "媒体文件",
            extensions: ["mp3", "m4a", "wav", "flac", "ogg", "mp4", "mov", "webm", "mkv"],
          },
        ],
      });
      if (Array.isArray(picked)) addPaths(picked);
      else if (typeof picked === "string") addPaths([picked]);
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const pickFolder = async () => {
    if (!isTauri) return notify("请在桌面应用中选择文件夹", "err");
    try {
      const dir = await open({ directory: true, multiple: false });
      if (typeof dir !== "string") return;
      const found = await api.scanFolder(dir);
      setFiles((prev) => {
        const known = new Set(prev.map((f) => f.path));
        return [...prev, ...found.filter((f) => !known.has(f.path))];
      });
      const auto = found.filter((f) => !f.hasMp3Sibling).map((f) => f.path);
      setSelected((s) => new Set([...s, ...auto]));
      notify(`扫描到 ${found.length} 个媒体文件`);
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const toggle = (path: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  };

  const start = async () => {
    const paths = [...selected];
    if (!paths.length) return notify("请先选择要转录的文件", "err");
    try {
      await api.startTranscribe(paths, { model, language });
      notify(`已提交 ${paths.length} 个转录任务`);
      setFiles([]);
      setSelected(new Set());
    } catch (e) {
      notify(String(e), "err");
    }
  };

  const modelOptions = (status?.models ?? []).map((m) => ({
    value: m.id,
    label: `${m.label}${m.installed ? "" : " · 未下载"}`,
  }));

  const transcribeTasks = tasks.filter((t) => t.kind === "transcribe");

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto p-5">
        <div
          onDragOver={(e) => {
            e.preventDefault();
            setDragging(true);
          }}
          onDragLeave={() => setDragging(false)}
          onDrop={(e) => {
            e.preventDefault();
            setDragging(false);
            if (isTauri) return;
            const list = Array.from(e.dataTransfer.files).map((f) => (f as File & { path?: string }).path ?? f.name);
            addPaths(list);
          }}
          className={`flex flex-col items-center justify-center gap-3 rounded-xl border-2 border-dashed px-6 py-9 text-center transition-colors ${
            dragging
              ? "border-primary/70 bg-primary/5"
              : "border-border/70 bg-card/40 hover:border-border"
          }`}
        >
          <div className="flex size-11 items-center justify-center rounded-xl bg-secondary/70">
            <UploadCloud className="size-5 text-primary" />
          </div>
          <div>
            <p className="text-sm font-medium">拖拽音频 / 视频到此处批量转录</p>
            <p className="mt-1 text-[11px] text-muted-foreground">
              支持 mp3 / m4a / wav / mp4 / mov / webm / mkv，自动抽取 16k 单声道音频
            </p>
          </div>
          <div className="flex gap-2">
            <Button variant="secondary" size="sm" onClick={pickFiles}>
              <Plus className="size-3.5" /> 选择文件
            </Button>
            <Button variant="secondary" size="sm" onClick={pickFolder}>
              <FolderSearch className="size-3.5" /> 扫描文件夹
            </Button>
          </div>
        </div>

        {files.length > 0 && (
          <div className="mt-4 overflow-hidden rounded-xl border border-border/70">
            <div className="flex items-center justify-between border-b border-border/60 bg-secondary/30 px-4 py-2 text-[11px] text-muted-foreground">
              <div>
                已选择 <span className="font-semibold text-foreground">{selected.size}</span> /{" "}
                {files.length}
              </div>
              <div className="flex items-center gap-2">
                <button
                  className="hover:text-foreground"
                  onClick={() => setSelected(new Set(files.map((f) => f.path)))}
                >
                  全选
                </button>
                <span className="text-border">|</span>
                <button className="hover:text-foreground" onClick={() => setSelected(new Set())}>
                  清空选择
                </button>
                <span className="text-border">|</span>
                <button
                  className="flex items-center gap-1 hover:text-red-300"
                  onClick={() => {
                    setFiles([]);
                    setSelected(new Set());
                  }}
                >
                  <Trash2 className="size-3" /> 清空
                </button>
              </div>
            </div>
            <div className="max-h-64 overflow-y-auto">
              {files.map((f) => {
                const checked = selected.has(f.path);
                return (
                  <button
                    key={f.path}
                    onClick={() => toggle(f.path)}
                    className="flex w-full items-center gap-3 border-b border-border/40 px-4 py-2.5 text-left last:border-0 hover:bg-accent/30"
                  >
                    <span
                      className={`flex size-4 shrink-0 items-center justify-center rounded border ${
                        checked ? "border-primary bg-primary text-primary-foreground" : "border-border"
                      }`}
                    >
                      {checked && <Check className="size-3" />}
                    </span>
                    <FileAudio className="size-3.5 shrink-0 text-muted-foreground" />
                    <span className="min-w-0 flex-1 truncate text-xs">{f.name}</span>
                    {f.hasMp3Sibling && (
                      <span className="rounded-full bg-amber-500/15 px-2 py-0.5 text-[10px] text-amber-300">
                        已有 MP3
                      </span>
                    )}
                    {f.size > 0 && (
                      <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
                        {formatBytes(f.size)}
                      </span>
                    )}
                    <X
                      className="size-3.5 shrink-0 text-muted-foreground/50 hover:text-red-400"
                      onClick={(e) => {
                        e.stopPropagation();
                        setFiles((prev) => prev.filter((x) => x.path !== f.path));
                        setSelected((prev) => {
                          const next = new Set(prev);
                          next.delete(f.path);
                          return next;
                        });
                      }}
                    />
                  </button>
                );
              })}
            </div>
          </div>
        )}

        <div className="mt-4 grid grid-cols-2 gap-3">
          <div className="space-y-1.5">
            <label className="text-[11px] text-muted-foreground">Whisper 模型</label>
            <Select
              value={model}
              onValueChange={setModel}
              options={
                modelOptions.length
                  ? modelOptions
                  : [{ value: model, label: model }]
              }
            />
          </div>
          <div className="space-y-1.5">
            <label className="text-[11px] text-muted-foreground">语言</label>
            <Select value={language} onValueChange={setLanguage} options={LANG_OPTIONS} />
          </div>
        </div>

        <div className="mt-4 flex items-center justify-between rounded-xl border border-border/60 bg-card/40 px-4 py-3">
          <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
            <Sparkles className="size-3.5 text-primary" />
            字幕将输出到媒体文件同目录（同名 .srt）
          </div>
          <Button onClick={start} disabled={!selected.size} className="gap-2">
            <AudioLines className="size-4" />
            开始转录
          </Button>
        </div>

        {transcribeTasks.length > 0 && (
          <div className="mt-5 space-y-2">
            <div className="text-[11px] font-medium text-muted-foreground">转录任务</div>
            {[...transcribeTasks].reverse().map((t) => {
              const meta = stageMeta[t.stage];
              return (
                <div
                  key={t.id}
                  className="rounded-xl border border-border/70 bg-card px-4 py-3"
                >
                  <div className="flex items-center gap-2">
                    <span className="truncate text-xs font-medium">{t.title}</span>
                    <span
                      className={`rounded-full px-2 py-0.5 text-[10px] ${meta.className}`}
                    >
                      {meta.label}
                    </span>
                    <span className="ml-auto font-mono text-[10px] text-muted-foreground">
                      {t.progress.toFixed(0)}%
                    </span>
                  </div>
                  <div className="mt-2 flex items-center gap-3">
                    <Progress value={t.progress} className="flex-1" />
                    {isActive(t.stage) && (
                      <button
                        className="text-[10px] text-red-400 hover:underline"
                        onClick={() => api.cancelTask(t.id)}
                      >
                        取消
                      </button>
                    )}
                    {t.srtPath && (
                      <button
                        className="text-[10px] text-emerald-400 hover:underline"
                        onClick={() => api.openPath(t.srtPath!)}
                      >
                        打开字幕
                      </button>
                    )}
                  </div>
                  <div className="mt-1 text-[10px] text-muted-foreground">{t.message}</div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
