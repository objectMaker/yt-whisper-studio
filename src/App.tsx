import { useCallback, useEffect, useState } from "react";
import {
  AudioLines,
  CircleAlert,
  CircleCheck,
  FolderOpen,
  Globe2,
  ListVideo,
  Settings2,
  Wrench,
} from "lucide-react";
import { api, type AssetProgress, type Settings, type Task, type ToolchainStatus } from "@/lib/api";
import { cn } from "@/lib/utils";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { isActive } from "@/lib/stage";
import BrowserPage from "@/pages/BrowserPage";
import QueuePage from "@/pages/QueuePage";
import TranscribePage from "@/pages/TranscribePage";
import ToolsPage from "@/pages/ToolsPage";

export type Tab = "browser" | "queue" | "transcribe" | "tools";

const NAV: { id: Tab; label: string; icon: typeof Globe2 }[] = [
  { id: "browser", label: "YouTube 浏览器", icon: Globe2 },
  { id: "queue", label: "下载 & 转录队列", icon: ListVideo },
  { id: "transcribe", label: "本地批量转录", icon: AudioLines },
  { id: "tools", label: "工具链与设置", icon: Wrench },
];

export default function App() {
  const [tab, setTab] = useState<Tab>("browser");
  const [settings, setSettings] = useState<Settings | null>(null);
  const [status, setStatus] = useState<ToolchainStatus | null>(null);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [assets, setAssets] = useState<Record<string, AssetProgress>>({});
  const [toast, setToast] = useState<{ msg: string; kind: "ok" | "err" } | null>(null);

  const notify = useCallback((msg: string, kind: "ok" | "err" = "ok") => {
    setToast({ msg, kind });
  }, []);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await api.toolchainStatus());
    } catch (e) {
      notify(String(e), "err");
    }
  }, [notify]);

  useEffect(() => {
    api.getSettings().then(setSettings).catch((e) => notify(String(e), "err"));
    api.listTasks().then(setTasks).catch(() => {});
    refreshStatus();
  }, [notify, refreshStatus]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 3200);
    return () => clearTimeout(t);
  }, [toast]);

  useEffect(() => {
    const unsubs: (() => void)[] = [];
    api
      .onTaskUpdated((t) =>
        setTasks((prev) => {
          const idx = prev.findIndex((x) => x.id === t.id);
          if (idx === -1) return [...prev, t];
          const next = [...prev];
          next[idx] = t;
          return next;
        }),
      )
      .then((u) => unsubs.push(u));
    api
      .onAssetProgress((p) =>
        setAssets((prev) => ({ ...prev, [`${p.kind}:${p.id}`]: p })),
      )
      .then((u) => unsubs.push(u));
    api
      .onTaskRemoved((id) => setTasks((prev) => prev.filter((x) => x.id !== id)))
      .then((u) => unsubs.push(u));
    return () => unsubs.forEach((u) => u());
  }, []);

  const saveSettings = useCallback(
    async (next: Settings) => {
      try {
        const saved = await api.saveSettings(next);
        setSettings(saved);
        notify("设置已保存");
      } catch (e) {
        notify(String(e), "err");
      }
    },
    [notify],
  );

  const startDownload = useCallback(
    async (url: string, options: { extractMp3?: boolean; transcribe?: boolean }) => {
      try {
        await api.startDownload(url, options);
        setTab("queue");
        notify("任务已加入队列");
      } catch (e) {
        notify(String(e), "err");
      }
    },
    [notify],
  );

  const activeCount = tasks.filter((t) => isActive(t.stage)).length;

  return (
    <div className="flex h-screen w-screen overflow-hidden bg-background text-foreground">
      <aside className="flex w-[228px] shrink-0 flex-col border-r border-border/70 bg-[linear-gradient(180deg,oklch(0.175_0.012_265),oklch(0.145_0.008_265))]">
        <div className="flex items-center gap-3 px-5 pb-5 pt-6">
          <div className="flex size-9 items-center justify-center rounded-xl bg-primary text-primary-foreground shadow-lg shadow-primary/25">
            <svg viewBox="0 0 24 24" className="size-4" fill="currentColor">
              <path d="M8 5.5v13l11-6.5-11-6.5Z" />
            </svg>
          </div>
          <div>
            <div className="text-[13px] font-semibold leading-tight tracking-tight">
              YT-Whisper
            </div>
            <div className="text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
              Studio
            </div>
          </div>
        </div>

        <nav className="flex flex-1 flex-col gap-1 px-3">
          {NAV.map((item) => {
            const Icon = item.icon;
            const selected = tab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => setTab(item.id)}
                className={cn(
                  "group flex items-center gap-3 rounded-lg px-3 py-2 text-[13px] transition-colors",
                  selected
                    ? "bg-accent text-foreground shadow-sm"
                    : "text-muted-foreground hover:bg-accent/50 hover:text-foreground",
                )}
              >
                <Icon
                  className={cn(
                    "size-4",
                    selected ? "text-primary" : "text-muted-foreground/80",
                  )}
                />
                <span className="flex-1 text-left">{item.label}</span>
                {item.id === "queue" && activeCount > 0 && (
                  <span className="rounded-full bg-primary/20 px-1.5 py-0.5 text-[10px] font-semibold text-primary">
                    {activeCount}
                  </span>
                )}
              </button>
            );
          })}
        </nav>

        <div className="space-y-2 border-t border-border/60 p-3">
          <button
            onClick={() => settings && api.openPath(settings.outputDir)}
            className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-[11px] text-muted-foreground transition hover:bg-accent/50 hover:text-foreground"
            title={settings?.outputDir}
          >
            <FolderOpen className="size-3.5 shrink-0" />
            <span className="truncate">{settings?.outputDir || "输出目录"}</span>
          </button>
          <div className="flex items-center justify-between px-3 text-[10px] text-muted-foreground/60">
            <span>v0.1.0</span>
            <span>whisper.cpp</span>
          </div>
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-border/60 px-6">
          <div className="flex items-center gap-3">
            <h1 className="text-sm font-semibold">
              {NAV.find((n) => n.id === tab)?.label}
            </h1>
            {tab === "browser" && (
              <Badge variant="outline" className="border-border/70">
                WebView2 内置会话
              </Badge>
            )}
          </div>
          <div className="flex items-center gap-3 text-xs">
            <div className="flex items-center gap-1.5 text-muted-foreground">
              <span
                className={cn(
                  "size-1.5 rounded-full",
                  status?.ready ? "bg-emerald-400" : "bg-amber-400",
                )}
              />
              {status?.ready ? "工具链就绪" : "工具链待补齐"}
            </div>
            <Button
              variant="ghost"
              size="icon-sm"
              onClick={() => setTab("tools")}
              title="设置"
            >
              <Settings2 className="size-4" />
            </Button>
          </div>
        </header>

        <div className="min-h-0 flex-1">
          {tab === "browser" && (
            <BrowserPage settings={settings} onStartDownload={startDownload} notify={notify} />
          )}
          {tab === "queue" && (
            <QueuePage tasks={tasks} settings={settings} notify={notify} />
          )}
          {tab === "transcribe" && (
            <TranscribePage tasks={tasks} status={status} settings={settings} notify={notify} />
          )}
          {tab === "tools" && (
            <ToolsPage
              settings={settings}
              status={status}
              assets={assets}
              onChangeSettings={setSettings}
              onSaveSettings={saveSettings}
              onRefreshStatus={refreshStatus}
              notify={notify}
            />
          )}
        </div>
      </main>

      {toast && (
        <div
          className={cn(
            "fixed bottom-5 right-5 z-[100] flex items-center gap-2 rounded-lg border px-3.5 py-2.5 text-xs shadow-2xl",
            toast.kind === "ok"
              ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-300"
              : "border-red-500/30 bg-red-500/10 text-red-300",
          )}
        >
          {toast.kind === "ok" ? (
            <CircleCheck className="size-3.5" />
          ) : (
            <CircleAlert className="size-3.5" />
          )}
          {toast.msg}
        </div>
      )}
    </div>
  );
}
