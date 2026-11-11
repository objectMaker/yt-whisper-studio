import type { TaskStage } from "@/lib/api";

export const stageMeta: Record<
  TaskStage,
  { label: string; className: string }
> = {
  queued: { label: "排队中", className: "bg-zinc-500/15 text-zinc-300" },
  preparing: { label: "准备模型", className: "bg-violet-500/15 text-violet-300" },
  downloading: { label: "下载中", className: "bg-blue-500/15 text-blue-300" },
  extracting: { label: "音频处理", className: "bg-amber-500/15 text-amber-300" },
  transcribing: { label: "转录中", className: "bg-fuchsia-500/15 text-fuchsia-300" },
  done: { label: "已完成", className: "bg-emerald-500/15 text-emerald-300" },
  error: { label: "失败", className: "bg-red-500/15 text-red-300" },
  paused: { label: "已暂停", className: "bg-amber-500/15 text-amber-300" },
  canceled: { label: "已取消", className: "bg-zinc-500/15 text-zinc-400" },
};

export function isActive(stage: TaskStage) {
  return ["queued", "preparing", "downloading", "extracting", "transcribing"].includes(
    stage,
  );
}

export function canRetry(task: { stage: TaskStage; url: string | null; filePath: string | null }) {
  return (
    ["error", "paused", "canceled"].includes(task.stage) &&
    Boolean(task.url || task.filePath)
  );
}
