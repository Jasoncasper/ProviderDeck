import { useEffect, useRef } from "react";
import { X } from "lucide-react";

interface ContactDialogProps {
  open: boolean;
  onClose: () => void;
}

export function ContactDialog({ open, onClose }: ContactDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handleKey);
    return () => document.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  useEffect(() => {
    if (!open) return;
    const handleClick = (e: MouseEvent) => {
      if (dialogRef.current && !dialogRef.current.contains(e.target as Node)) {
        onClose();
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
      <div
        ref={dialogRef}
        className="relative rounded-lg border bg-card text-card-foreground shadow-lg p-6 flex flex-col items-center gap-4"
        style={{ maxWidth: 360 }}
      >
        <button
          onClick={onClose}
          className="absolute top-3 right-3 rounded-md p-1 text-muted-foreground hover:text-foreground hover:bg-accent"
          type="button"
        >
          <X className="h-4 w-4" />
        </button>
        <h3 className="text-base font-semibold">联系我</h3>
        <p className="text-sm text-muted-foreground">扫描微信(wechat)二维码添加好友</p>
        <img
          src="/callme.jpg"
          alt="微信二维码"
          className="rounded-md"
          style={{ width: 240, height: 240 }}
        />
      </div>
    </div>
  );
}
