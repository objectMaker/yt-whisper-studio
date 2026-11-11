import { cn } from "@/lib/utils";

export function Progress({
  value,
  className,
  indeterminate,
}: {
  value: number;
  className?: string;
  indeterminate?: boolean;
}) {
  return (
    <div
      className={cn(
        "relative h-1.5 w-full overflow-hidden rounded-full bg-secondary",
        className,
      )}
    >
      <div
        className={cn(
          "h-full rounded-full bg-gradient-to-r from-primary/80 to-primary transition-[width] duration-300",
          indeterminate && "animate-pulse",
        )}
        style={{ width: `${Math.max(0, Math.min(100, value))}%` }}
      />
    </div>
  );
}
