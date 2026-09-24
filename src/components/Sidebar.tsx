import { Boxes, Globe2, Gamepad2, Settings2, TerminalSquare } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useApp } from "../state/app";

export type View = "dashboard" | "instances" | "catalog" | "console" | "settings";

const NAV: Array<{ id: View; label: string; icon: LucideIcon }> = [
  { id: "dashboard", label: "Играть", icon: Gamepad2 },
  { id: "instances", label: "Инстансы", icon: Boxes },
  { id: "catalog", label: "Моды Modrinth", icon: Globe2 },
  { id: "console", label: "Консоль", icon: TerminalSquare },
  { id: "settings", label: "Настройки", icon: Settings2 },
];

export function Sidebar({ view, setView }: { view: View; setView: (v: View) => void }) {
  const { status, logs, sys } = useApp();

  return (
    <aside className="glass-strong z-10 m-2 mr-0 flex w-56 shrink-0 flex-col gap-1 p-3">
      {NAV.map((n) => (
        <button
          key={n.id}
          className={`nav-item ${view === n.id ? "active" : ""}`}
          onClick={() => setView(n.id)}
        >
          <n.icon size={17} />
          <span className="text-[13px] font-medium">{n.label}</span>
          {n.id === "console" && logs.length > 0 && (
            <span className="badge badge-accent ml-auto">{logs.length > 999 ? "999+" : logs.length}</span>
          )}
          {n.id === "dashboard" && status?.running && <span className="dot dot-live ml-auto" />}
        </button>
      ))}
      <div className="flex-1" />
      <div className="glass p-3 text-[11px] leading-relaxed" style={{ color: "var(--text-dim)" }}>
        <div className="font-semibold" style={{ color: "var(--text)" }}>
          <span className="dot dot-live mr-1.5 inline-block" style={{ width: 6, height: 6 }} />
          {sys?.os ?? "…"}
        </div>
        {[sys?.kernel, sys?.arch].filter(Boolean).join(" · ")}
        <br />
        Tauri 2 · {sys?.renderer ?? "…"}
        <br />
        deBang v{sys?.launcherVersion ?? "1.2"}
      </div>
    </aside>
  );
}
