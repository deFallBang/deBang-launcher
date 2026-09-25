import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { useApp } from "../state/app";

export function TitleBar() {
  const { sys, toast } = useApp();
  const [expanded, setExpanded] = useState(false);
  const [minimizable, setMinimizable] = useState(true);
  const win = getCurrentWindow();
  // On Linux the compositor can ignore maximize/minimize entirely (verified:
  // set_size / maximize / minimize are dropped, only fullscreen and hide work),
  // so those controls are not shown at all — a dead button is worse than none.
  const isLinux = sys
    ? sys.session === "wayland" || sys.os.toLowerCase().includes("linux")
    : typeof navigator !== "undefined" && navigator.userAgent.toLowerCase().includes("linux");

  const syncState = useCallback(async () => {
    const [maxed, full] = await Promise.all([
      win.isMaximized().catch(() => false),
      win.isFullscreen().catch(() => false),
    ]);
    setExpanded(maxed || full);
  }, [win]);

  useEffect(() => {
    void syncState();
    const un = win.onResized(() => void syncState());
    return () => {
      void un.then((f) => f()).catch(() => {});
    };
  }, [win, syncState]);

  // Esc always leaves fullscreen, even if the WM does not wire it up.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        void win.isFullscreen().then((f) => {
          if (f) void win.setFullscreen(false);
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [win]);

  const toggleExpand = useCallback(async () => {
    const maxed = await win.isMaximized().catch(() => false);
    if (maxed) {
      await win.unmaximize();
      setExpanded(false);
      return;
    }
    const before = await win.outerSize().then((s) => s.width).catch(() => 0);
    await win.maximize();
    await new Promise((r) => setTimeout(r, 250));
    const after = await win.outerSize().then((s) => s.width).catch(() => 0);
    if (!after || after === before) {
      // compositor ignored maximize — use fullscreen instead
      await win.setFullscreen(true).catch(() => {});
    }
    setExpanded(true);
  }, [win]);

  const doMinimize = useCallback(async () => {
    await win.minimize().catch(() => {});
    await new Promise((r) => setTimeout(r, 250));
    const isMin = await win.isMinimized().catch(() => false);
    if (!isMin) {
      setMinimizable(false);
      toast("Свертывание не поддерживается этим композитором — кнопка убрана", "err");
    }
  }, [win, toast]);

  return (
    <div className="flex h-11 shrink-0 items-center gap-3 px-4">
      {/* drag regions live in their own elements: a drag-region parent would
          swallow clicks on the window control buttons */}
      <div className="flex shrink-0 items-center gap-3" data-tauri-drag-region>
        <div className="grid size-6 place-items-center rounded-lg" style={{ background: "var(--accent)" }}>
          <span className="text-[11px] font-black" style={{ color: "var(--accent-fg)" }}>dB</span>
        </div>
        <span className="text-[13px] font-semibold tracking-wide opacity-90">
          de<span style={{ color: "var(--accent)" }}>Bang</span> Launcher
        </span>
        <span className="badge ml-1 hidden md:inline-block">V1.3.1</span>
      </div>
      <div
        className="h-full flex-1"
        data-tauri-drag-region
        onDoubleClick={() => {
          if (!isLinux) void toggleExpand();
        }}
      />
      <div className="flex items-center gap-1">
        {!isLinux && minimizable && (
          <button
            className="wctl"
            title="Свернуть"
            aria-label="Свернуть"
            onClick={() => void doMinimize()}
          >
            <Minus size={15} />
          </button>
        )}
        {!isLinux && (
        <button
          className="wctl"
          title={expanded ? "Восстановить" : "Развернуть"}
          aria-label={expanded ? "Восстановить окно" : "Развернуть окно"}
          onClick={() => void toggleExpand()}
        >
          {expanded ? <Copy size={13} style={{ transform: "rotate(90deg)" }} /> : <Square size={13} />}
        </button>
        )}
        <button
          className="wctl close"
          title="Закрыть"
          aria-label="Закрыть"
          onClick={() => void win.close()}
        >
          <X size={15} />
        </button>
      </div>
    </div>
  );
}
