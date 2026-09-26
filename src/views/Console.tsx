import { useEffect, useMemo, useRef, useState } from "react";
import { Eraser, Radio, Search } from "lucide-react";
import { useApp } from "../state/app";

const STREAMS = [
  { id: "stdout", label: "out" },
  { id: "stderr", label: "err" },
  { id: "launcher", label: "sys" },
];

export function Console() {
  const { logs, clearLogs, status } = useApp();
  const [filter, setFilter] = useState("");
  const [on, setOn] = useState({ stdout: true, stderr: true, launcher: true });
  const [auto, setAuto] = useState(true);
  const endRef = useRef<HTMLDivElement>(null);

  const shown = useMemo(() => {
    const f = filter.toLowerCase();
    return logs.filter(
      (l) => on[l.stream as keyof typeof on] && (!f || l.line.toLowerCase().includes(f)),
    );
  }, [logs, filter, on]);

  const lastKey = shown.length ? `${shown[shown.length - 1].time}:${shown.length}` : "";
  useEffect(() => {
    if (auto) endRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [lastKey, auto]);

  return (
    <div className="view-enter flex h-full flex-col gap-3 p-6 pt-3">
      <div className="flex items-center gap-3">
        <h2 className="flex items-center gap-2 text-2xl font-bold">
          {status?.running && <span className="dot dot-live" />} Консоль
        </h2>
        <span className="text-[11.5px] opacity-45">
          stdout / stderr дочернего процесса в реальном времени
        </span>
        <div className="flex-1" />
        {STREAMS.map((s) => (
          <button
            key={s.id}
            className={`badge cursor-pointer !py-1 ${on[s.id as keyof typeof on] ? "badge-accent" : "opacity-40"}`}
            onClick={() => setOn((o) => ({ ...o, [s.id]: !o[s.id as keyof typeof o] }))}
          >
            {s.label}
          </button>
        ))}
        <button
          className={`badge cursor-pointer !py-1 ${auto ? "badge-accent" : "opacity-40"}`}
          onClick={() => setAuto((a) => !a)}
        >
          автоскролл
        </button>
      </div>

      <div className="flex gap-2">
        <div className="relative flex-1">
          <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2 opacity-50" />
          <input
            className="inp font-mono-console !pl-9 !text-[12px]"
            placeholder="Фильтр по строкам лога…"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>
        <button className="btn" onClick={clearLogs}>
          <Eraser size={14} /> Очистить
        </button>
      </div>

      <div className="console-scroll glass-strong flex-1 overflow-y-auto p-2 font-mono-console text-[11.5px] leading-[1.75]">
        {shown.length === 0 ? (
          <div className="grid h-full place-items-center gap-2 opacity-50">
            <Radio size={26} style={{ color: "var(--accent)" }} />
            <p className="text-[12.5px]">
              {logs.length === 0 ? "Логи появятся здесь после запуска версии" : "Нет строк по фильтру"}
            </p>
          </div>
        ) : (
          shown.map((l, i) => (
            <div
              key={i}
              className={`console-line ${
                /\b(ERROR|SEVERE)\b/.test(l.line)
                  ? "ln-error"
                  : l.stream === "stderr"
                    ? "ln-stderr"
                    : l.stream === "launcher"
                      ? "ln-launcher"
                      : "ln-stdout"
              }`}
            >
              <span className="mr-2 opacity-40 select-none">
                {new Date(l.time).toLocaleTimeString("ru", { hour12: false })}
              </span>
              {l.line}
            </div>
          ))
        )}
        <div ref={endRef} />
      </div>
    </div>
  );
}
