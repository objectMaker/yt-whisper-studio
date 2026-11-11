import { useCallback, useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ArrowRight,
  Captions,
  Download,
  Globe2,
  Loader2,
  LockKeyhole,
  Music4,
  RefreshCw,
  ShieldCheck,
  Sparkles,
} from "lucide-react";
import { api, isTauri, type CookieSummary, type Settings } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Switch } from "@/components/ui/switch";
import { Modal } from "@/components/ui/dialog";
import { cn } from "@/lib/utils";

function youtubeVideoId(u: string | null): string | null {
  if (!u) return null;
  try {
    const url = new URL(u);
    if (url.hostname.endsWith("youtube.com")) {
      const v = url.searchParams.get("v");
      if (v) return v;
      if (url.pathname.startsWith("/shorts/")) return url.pathname.split("/")[2] || null;
      if (url.pathname.startsWith("/live/")) return url.pathname.split("/")[2] || null;
    }
    if (url.hostname === "youtu.be") return url.pathname.slice(1) || null;
  } catch {
    /* ignore */
  }
  return null;
}

export default function BrowserPage({
  settings,
  onStartDownload,
  notify,
}: {
  settings: Settings | null;
  onStartDownload: (url: string, o: { extractMp3?: boolean; transcribe?: boolean }) => void;
  notify: (msg: string, kind?: "ok" | "err") => void;
}) {
  const holderRef = useRef<HTMLDivElement>(null);
  const addressFocused = useRef(false);
  const [address, setAddress] = useState("https://www.youtube.com/");
  const [currentUrl, setCurrentUrl] = useState<string | null>(null);
  const [videoId, setVideoId] = useState<string | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [target, setTarget] = useState<string | null>(null);
  const [extractMp3, setExtractMp3] = useState(settings?.extractMp3 ?? true);
  const [transcribe, setTranscribe] = useState(settings?.transcribe ?? true);
  const [login, setLogin] = useState<CookieSummary | null>(null);
  const [checking, setChecking] = useState(false);
  const [ready, setReady] = useState(false);

  const syncBounds = useCallback(() => {
    const el = holderRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    if (r.width < 10 || r.height < 10) return;
    api
      .browserSetBounds({ x: r.left, y: r.top, width: r.width, height: r.height })
      .catch(() => {});
  }, []);

  useEffect(() => {
    const el = holderRef.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    api
      .browserShow({ x: r.left, y: r.top, width: r.width, height: r.height })
      .then(() => setReady(true))
      .catch((e) => notify(String(e), "err"));
    const ro = new ResizeObserver(() => syncBounds());
    ro.observe(el);
    window.addEventListener("resize", syncBounds);
    return () => {
      ro.disconnect();
      window.removeEventListener("resize", syncBounds);
      api.browserHide().catch(() => {});
    };
  }, [notify, syncBounds]);

  useEffect(() => {
    if (!ready) return;
    if (modalOpen) {
      api.browserHide().catch(() => {});
    } else {
      const el = holderRef.current;
      if (!el) return;
      const r = el.getBoundingClientRect();
      api
        .browserShow({ x: r.left, y: r.top, width: r.width, height: r.height })
        .catch(() => {});
    }
  }, [modalOpen, ready]);

  useEffect(() => {
    if (!isTauri) return;
    const timer = setInterval(async () => {
      try {
        const u = await api.browserUrl();
        if (u) {
          setCurrentUrl(u);
          setVideoId(youtubeVideoId(u));
          if (!addressFocused.current) setAddress(u);
        }
      } catch {
        /* ignore */
      }
    }, 1500);
    return () => clearInterval(timer);
  }, []);

  const navigate = () => {
    let target = address.trim();
    if (!target) return;
    if (!/^https?:\/\//i.test(target)) {
      if (target.includes(".") && !target.includes(" ")) target = `https://${target}`;
      else target = `https://www.youtube.com/results?search_query=${encodeURIComponent(target)}`;
    }
    setAddress(target);
    api.browserNavigate(target).catch((e) => notify(String(e), "err"));
  };

  const checkLogin = async () => {
    setChecking(true);
    try {
      const res = await api.extractCookies();
      setLogin(res);
      notify(res.loggedIn ? `已获取 ${res.count} 条 Cookie（已登录）` : "已获取 Cookie，但未检测到登录态");
    } catch (e) {
      notify(String(e), "err");
    } finally {
      setChecking(false);
    }
  };

  const downloadUrl = videoId ? `https://www.youtube.com/watch?v=${videoId}` : currentUrl;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-border/60 px-4 py-2.5">
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="icon-sm" onClick={() => api.browserBack()}>
            <ArrowLeft className="size-4" />
          </Button>
          <Button variant="ghost" size="icon-sm" onClick={() => api.browserForward()}>
            <ArrowRight className="size-4" />
          </Button>
          <Button variant="ghost" size="icon-sm" onClick={() => api.browserReload()}>
            <RefreshCw className="size-3.5" />
          </Button>
        </div>
        <form
          className="relative flex-1"
          onSubmit={(e) => {
            e.preventDefault();
            navigate();
          }}
        >
          <Globe2 className="pointer-events-none absolute left-2.5 top-1/2 size-3.5 -translate-y-1/2 text-muted-foreground/60" />
          <Input
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            onFocus={() => (addressFocused.current = true)}
            onBlur={() => (addressFocused.current = false)}
            className="h-8 pl-8 pr-16 text-xs"
            spellCheck={false}
          />
          <button
            type="submit"
            className="absolute right-1 top-1/2 flex h-6 -translate-y-1/2 items-center rounded-md bg-secondary px-2 text-[11px] text-muted-foreground transition hover:text-foreground"
            title="打开地址"
          >
            前往
          </button>
        </form>
        <button
          onClick={checkLogin}
          disabled={checking}
          className={cn(
            "flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-[11px] transition",
            login?.loggedIn
              ? "border-emerald-500/30 bg-emerald-500/10 text-emerald-300"
              : "border-border bg-secondary/60 text-muted-foreground hover:text-foreground",
          )}
          title="从内置浏览器会话读取 YouTube Cookie"
        >
          {checking ? (
            <Loader2 className="size-3 animate-spin" />
          ) : login?.loggedIn ? (
            <ShieldCheck className="size-3.5" />
          ) : (
            <LockKeyhole className="size-3.5" />
          )}
          {login?.loggedIn ? `已登录 · ${login.count} cookies` : "检测登录态"}
        </button>
      </div>

      <div className="relative min-h-0 flex-1 p-3">
        <div
          ref={holderRef}
          className="relative h-full w-full overflow-hidden rounded-xl border border-border/70 bg-black/50"
        >
          {!isTauri && (
            <div className="flex h-full flex-col items-center justify-center gap-4 bg-[radial-gradient(circle_at_50%_30%,oklch(0.3_0.08_25/0.5),transparent_60%)] text-center">
              <div className="flex size-14 items-center justify-center rounded-2xl bg-primary/15">
                <Globe2 className="size-6 text-primary" />
              </div>
              <div className="space-y-1">
                <p className="text-sm font-medium">内置 YouTube 浏览器</p>
                <p className="max-w-sm text-xs leading-relaxed text-muted-foreground">
                  桌面应用内会在此加载 WebView2 页面，可直接登录 YouTube，Cookie
                  自动复用，无需手动导入。
                </p>
              </div>
            </div>
          )}
        </div>
      </div>

      <div className="flex h-[112px] shrink-0 items-center gap-4 border-t border-border/60 bg-card/40 px-5">
        {videoId ? (
          <>
            <img
              src={`https://i.ytimg.com/vi/${videoId}/mqdefault.jpg`}
              alt=""
              className="h-[68px] w-[120px] shrink-0 rounded-lg border border-border/60 object-cover"
            />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <Badge variant="success" className="rec-pulse">
                  已识别视频
                </Badge>
                <span className="font-mono text-[10px] text-muted-foreground">{videoId}</span>
              </div>
              <p className="mt-1.5 truncate text-xs text-muted-foreground" title={downloadUrl ?? ""}>
                {downloadUrl}
              </p>
              <div className="mt-1.5 flex items-center gap-3 text-[11px] text-muted-foreground/80">
                <span className="flex items-center gap-1">
                  <Music4 className="size-3" /> MP3 {settings?.extractMp3 ? "开" : "关"}
                </span>
                <span className="flex items-center gap-1">
                  <Captions className="size-3" /> 字幕 {settings?.transcribe ? "开" : "关"}
                </span>
                <span className="flex items-center gap-1">
                  <Sparkles className="size-3" /> {settings?.model}
                </span>
              </div>
            </div>
            <Button
              className="gap-2"
              onClick={() => {
                setExtractMp3(settings?.extractMp3 ?? true);
                setTranscribe(settings?.transcribe ?? true);
                setTarget(downloadUrl);
                setModalOpen(true);
              }}
            >
              <Download className="size-4" />
              下载此视频
            </Button>
          </>
        ) : (
          <>
            <div className="flex size-12 items-center justify-center rounded-xl border border-dashed border-border text-muted-foreground">
              <Globe2 className="size-5" />
            </div>
            <div className="flex-1">
              <p className="text-xs font-medium">浏览 YouTube，识别到视频后可一键下载</p>
              <p className="mt-1 text-[11px] text-muted-foreground">
                支持 watch / shorts / live 链接，自动复用当前登录会话。
              </p>
            </div>
          </>
        )}
      </div>

      <Modal
        open={modalOpen}
        onOpenChange={setModalOpen}
        title="下载并处理视频"
        description={target ?? ""}
        footer={
          <>
            <Button variant="ghost" onClick={() => setModalOpen(false)}>
              取消
            </Button>
            <Button
              onClick={() => {
                if (!target) return;
                onStartDownload(target, { extractMp3, transcribe });
                setModalOpen(false);
              }}
            >
              加入队列
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <label className="flex items-center justify-between rounded-lg border border-border/70 bg-secondary/30 px-4 py-3">
            <div>
              <div className="text-xs font-medium">提取 MP3 音频</div>
              <div className="mt-0.5 text-[11px] text-muted-foreground">
                从视频中导出高质量 MP3
              </div>
            </div>
            <Switch checked={extractMp3} onCheckedChange={setExtractMp3} />
          </label>
          <label className="flex items-center justify-between rounded-lg border border-border/70 bg-secondary/30 px-4 py-3">
            <div>
              <div className="text-xs font-medium">生成 SRT 字幕</div>
              <div className="mt-0.5 text-[11px] text-muted-foreground">
                使用本地 whisper.cpp（{settings?.model}）
              </div>
            </div>
            <Switch checked={transcribe} onCheckedChange={setTranscribe} />
          </label>
        </div>
      </Modal>
    </div>
  );
}
