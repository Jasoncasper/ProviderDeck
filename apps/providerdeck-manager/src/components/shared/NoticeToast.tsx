import { useEffect, useState } from "react";
import { X } from "lucide-react";

export interface NoticeToastProps {
  message: string;
  title?: string;
  variant: "success" | "error";
  autoDismiss?: boolean;
  dismissMs?: number;
  onDismiss?: () => void;
}

export function NoticeToast({
  message,
  title,
  variant,
  autoDismiss = true,
  dismissMs = 3500,
  onDismiss,
}: NoticeToastProps) {
  const [visible, setVisible] = useState(true);

  useEffect(() => {
    if (!autoDismiss) return;
    const timer = window.setTimeout(() => {
      setVisible(false);
      onDismiss?.();
    }, dismissMs);
    return () => window.clearTimeout(timer);
  }, [autoDismiss, dismissMs, onDismiss]);

  if (!visible) return null;

  const colors =
    variant === "success"
      ? "bg-green-50 dark:bg-green-950 text-green-800 dark:text-green-200 border-green-200 dark:border-green-800"
      : "bg-red-50 dark:bg-red-950 text-red-800 dark:text-red-200 border-red-200 dark:border-red-800";

  return (
    <div className={`m-4 p-3 rounded-md text-sm border ${colors} animate-in fade-in zoom-in-95`}>
      <div className="flex items-start justify-between gap-2">
        <div>
          {title && <strong className="mr-2">{title}</strong>}
          <span>{message}</span>
        </div>
        <button
          className="shrink-0 opacity-60 hover:opacity-100"
          onClick={() => {
            setVisible(false);
            onDismiss?.();
          }}
          type="button"
        >
          <X className="h-4 w-4" />
        </button>
      </div>
    </div>
  );
}
