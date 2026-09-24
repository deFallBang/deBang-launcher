import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2 } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppProvider, useApp } from "./state/app";
import { Background } from "./components/Background";
import { TitleBar } from "./components/TitleBar";
import { Sidebar, type View } from "./components/Sidebar";
import { Dashboard } from "./views/Dashboard";
import { Instances } from "./views/Instances";
import { Catalog } from "./views/Catalog";
import { Console } from "./views/Console";
import { Settings } from "./views/Settings";

function Toasts() {
  const { toasts } = useApp();
  return (
    <div className="pointer-events-none fixed bottom-5 right-5 z-50 flex flex-col gap-2">
      {toasts.map((t) => (
        <div
          key={t.id}
          role={t.kind === "err" ? "alert" : "status"}
          className="toast glass-strong flex max-w-90 items-center gap-2.5 px-4 py-3 text-[13px] shadow-xl"
        >
          {t.kind === "err" ? (
            <AlertTriangle size={16} style={{ color: "var(--danger)" }} />
          ) : (
            <CheckCircle2 size={16} style={{ color: "var(--good)" }} />
          )}
          <span>{t.msg}</span>
        </div>
      ))}
    </div>
  );
}

function Shell() {
  const [view, setView] = useState<View>("dashboard");
  const [maxed, setMaxed] = useState(false);
  useEffect(() => {
    const win = getCurrentWindow();
    const sync = () => {
      void Promise.all([win.isMaximized(), win.isFullscreen().catch(() => false)])
        .then(([m, f]) => setMaxed(m || f))
        .catch(() => {});
    };
    sync();
    const un = win.onResized(sync);
    return () => {
      void un.then((f) => f()).catch(() => {});
    };
  }, []);

  return (
    <div className={`app-shell ${maxed ? "maximized" : ""}`}>
      <Background />
      <div className="relative z-10 flex h-full flex-col">
        <TitleBar />
        <div className="flex min-h-0 flex-1">
          <Sidebar view={view} setView={setView} />
          <main className="min-w-0 flex-1">
            {view === "dashboard" && <Dashboard go={setView as (v: "console" | "instances" | "settings") => void} />}
            {view === "instances" && <Instances />}
            {view === "catalog" && <Catalog />}
            {view === "console" && <Console />}
            {view === "settings" && <Settings />}
          </main>
        </div>
      </div>
      <Toasts />
    </div>
  );
}

export default function App() {
  return (
    <AppProvider>
      <Shell />
    </AppProvider>
  );
}
