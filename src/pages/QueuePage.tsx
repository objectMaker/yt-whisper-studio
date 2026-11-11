import { useState } from "react";
import {
  Captions,
  ChevronDown,
  CircleX,
  Download,
  FolderOpen,
  ListVideo,
  Square,
  Trash2,
} from "lucide-react";
import { api, type Settings, type Task } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Progress } from "@/components/ui/progress";
import { cn } from "@/lib/utils";
import { isActive, stageMeta } from "@/lib/stage";

function dirOf(p: string) {
  const idx = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return idx > 0 ? p.slice(0, idx) : p;
}

export default function QueuePage({
  tasks,
  settings,
  notify,
}: {
  tasks: Task[];
  settings: Settings | null;
  notify: (msg: string, kind?: "ok" | "err") => void;
}) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const active = tasks.filter((t) => isActive(t.stage)).length;
  const done = tasks.filter((t) => t.stage === "done").length;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-6 border-b border-border/60 px-6 py-3 text-xs">
        <div className="text-muted-foreground">
          进行中 <span className="ml-1 font-semibold text-foreground">{active}</span>
        </div>
        <div className="text-muted-foreground">
          已完成 <span className="ml-1 font-semibold text-emerald-400">{done}</span>
        </div>
        <div className="ml-auto text-[11px] text-muted-foreground/70">
          输出：{settings?.outputDir}
        </div>
      </div>

      <div className="min-h-0 flex-1 overflow-y-auto p-4">
        {tasks.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 text-center">
            <div className="flex size-14 items-center justify-center rounded-2xl bg-secondary/60">
              <ListVideo className="size-6 text-muted-foreground" />
            </div>
            <div>
              <p className="text-sm font-medium">队列为空</p>
              <p className="mt-1 text-xs text-muted-foreground">
                在「YouTube 浏览器」中打开视频，点击下载即可加入队列。
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-2.5">
            {[...tasks].reverse().map((task) => {
              const meta = stageMeta[task.stage];
              const isOpen = expanded === task.id;
              return (
                <div
                  key={task.id}
                  className="overflow-hidden rounded-xl border border-border/70 bg-card transition-colors hover:border-border"
                >
                  <div className="flex items-start gap-3 p-4">
                    <div className="mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-lg bg-secondary/70">
                      {task.kind === "download" ? (
                        <Download className="size-4 text-primary" />
                      ) : (
                        <Captions className="size-4 text-fuchsia-400" />
                      )}
                    </div>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="truncate text-[13px] font-medium">{task.title}</span>
                        <span
                          className={cn(
                            "shrink-0 rounded-full px-2 py-0.5 text-[10px] font-medium",
                            meta.className,
                          )}
                        >
                          {meta.label}
                        </span>
                        {task.model && (
                          <span className="shrink-0 rounded-full bg-secondary px-2 py-0.5 text-[10px] text-muted-foreground">
                            {task.model}
                          </span>
                        )}
                      </div>
                      <div className="mt-2 flex items-center gap-3">
                        <Progress value={task.progress} className="max-w-md flex-1" />
                        <span className="w-12 shrink-0 text-right font-mono text-[11px] text-muted-foreground">
                          {task.progress.toFixed(0)}%
                        </span>
                      </div>
                      <div className="mt-1.5 flex items-center gap-3 text-[11px] text-muted-foreground">
                        <span className="truncate">{task.message}</span>
                        {task.speed && <span className="font-mono">{task.speed}</span>}
                        {task.eta && task.eta !== "Unknown" && (
                          <span className="font-mono">剩余 {task.eta}</span>
                        )}
                      </div>
                      {task.error && (
                        <div className="mt-2 flex items-start gap-1.5 rounded-md border border-red-500/20 bg-red-500/5 px-2.5 py-1.5 text-[11px] text-red-300">
                          <CircleX className="mt-0.5 size-3 shrink-0" />
                          <span className="break-all">{task.error}</span>
                        </div>
                      )}
                      {task.srtPath && (
                        <button
                          onClick={() => api.openPath(task.srtPath!)}
                          className="mt-2 flex items-center gap-1.5 text-[11px] text-emerald-400 hover:underline"
                        >
                          <Captions className="size-3" />
                          {task.srtPath}
                        </button>
                      )}
                    </div>
                    <div className="flex shrink-0 items-center gap-1">
                      {isActive(task.stage) && (
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          title="取消"
                          onClick={() => api.cancelTask(task.id)}
                        >
                          <Square className="size-3.5 text-red-400" />
                        </Button>
                      )}
                      {task.filePath && task.stage === "done" && (
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          title="打开所在文件夹"
                          onClick={() => api.openPath(dirOf(task.filePath!))}
                        >
                          <FolderOpen className="size-3.5" />
                        </Button>
                      )}
                      {!isActive(task.stage) && (
                        <Button
                          variant="ghost"
                          size="icon-sm"
                          title="从列表移除"
                          onClick={() => {
                            api.removeTask(task.id);
                          }}
                        >
                          <Trash2 className="size-3.5 text-muted-foreground" />
                        </Button>
                      )}
                      <Button
                        variant="ghost"
                        size="icon-sm"
                        title="日志"
                        onClick={() => setExpanded(isOpen ? null : task.id)}
                      >
                        <ChevronDown
                          className={cn("size-3.5 transition-transform", isOpen && "rotate-180")}
                        />
                      </Button>
                    </div>
                  </div>
                  {isOpen && (
                    <div className="max-h-48 overflow-y-auto border-t border-border/60 bg-black/30 px-4 py-2.5 font-mono text-[10.5px] leading-relaxed text-muted-foreground">
                      {task.log.length === 0 ? (
                        <span className="text-muted-foreground/50">暂无日志</span>
                      ) : (
                        task.log.map((line, i) => <div key={i}>{line}</div>)
                      )}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
