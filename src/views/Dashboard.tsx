import { useState } from "react";
import { Coffee, Cpu, Download, Play, Rocket, Square, TerminalSquare, User } from "lucide-react";
import { api } from "../lib/api";
import { isValidPlayerName, JVM_PRESETS, useApp } from "../state/app";

function ramFmt(mb: number) {
  return mb >= 1024 ? `${(mb / 1024).toFixed(1)} ГБ` : `${mb} МБ`;
}

const PREP_LABELS: Record<string, string> = {
  manifest: "манифест версий",
  "version.json": "профиль версии",
  "fabric meta": "Fabric-метаданные",
  "client.jar": "клиентский jar",
  libraries: "библиотеки",
  assets: "ресурсы (звук/текстуры)",
};

export function Dashboard({ go }: { go: (v: "console" | "instances" | "settings") => void }) {
  const { settings, mem, memFailed, instances, status, prep, toast, patch, assembledArgs, sys } = useApp();
  const [busy, setBusy] = useState(false);
  const selected = instances.find((i) => i.config.id === settings.selectedInstance) ?? null;

  const usedPct = mem ? Math.min(100, ((mem.totalMb - mem.availableMb) / mem.totalMb) * 100) : 0;
  const nameOk = isValidPlayerName(settings.playerName);

  async function onPlay() {
    if (status?.running) {
      try {
        await api.stop(status.instanceId || settings.selectedInstance || "");
      } catch (e) {
        toast(String(e), "err");
      }
      return;
    }
    if (!selected) {
      toast("Сначала создайте версию", "err");
      go("instances");
      return;
    }
    if (!isValidPlayerName(settings.playerName)) {
      toast("Ник должен быть 3–16 символов: A–Z, 0–9, _", "err");
      go("settings");
      return;
    }
    if (!settings.javaPath && selected.hasRunScript) {
      toast("Для запуска run.sh укажите Java в настройках", "err");
      go("settings");
      return;
    }
    setBusy(true);
    try {
      await api.launch(selected.config.id, {
        javaPath: settings.javaPath,
        playerName: settings.playerName,
        minMemMb: settings.minMem,
        maxMemMb: settings.maxMem,
        jvmArgs: assembledArgs,
      });
      toast(`Запуск «${selected.config.name}»`);
    } catch (e) {
      toast(String(e), "err");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="flex h-full flex-col gap-5 overflow-y-auto p-6 pt-3">
      <div className="view-enter flex flex-1 flex-col items-center justify-center gap-6">
        <div className="text-center">
          <div className="text-[13px] uppercase tracking-[0.3em] opacity-50">
            {status?.running ? "Игра запущена" : "Готов к приключениям"}
          </div>
          <h1 className="mt-1 text-[40px] font-black leading-tight">
            {selected ? selected.config.name : "Версии пока нет"}
          </h1>
          {selected ? (
            <div className="mt-2 flex items-center justify-center gap-2 text-[13px] opacity-70">
              <span className="badge badge-accent">{selected.config.version}</span>
              <span className="badge">{selected.config.loader}</span>
              <span className="badge">{selected.modCount} модов</span>
            </div>
          ) : (
            <button className="btn btn-primary mt-3" onClick={() => go("instances")}>
              <Rocket size={15} /> Создать первую версию
            </button>
          )}
        </div>

        <div className="relative">
          {status?.running && (
            <span className="absolute -top-7 left-1/2 -translate-x-1/2 whitespace-nowrap text-[11px] font-mono-console" style={{ color: "var(--good)" }}>
              PID-процесс жив · поток логов активен
            </span>
          )}
          <button
            className={`play-orb ${status?.running ? "running" : ""}`}
            onClick={onPlay}
            disabled={busy}
            aria-label={status?.running ? "Остановить игру" : "Запустить игру"}
          >
            {busy ? (
              <span className="spinner" style={{ borderColor: "rgba(255,255,255,.3)", borderTopColor: "#fff" }} />
            ) : status?.running ? (
              <Square size={44} fill="currentColor" />
            ) : (
              <Play size={52} fill="currentColor" style={{ marginLeft: 6 }} />
            )}
          </button>
        </div>

        {prep && !status?.running && (
          <div className="glass w-full max-w-xl px-4 py-3 view-enter">
            <div className="mb-1.5 flex items-center justify-between text-[12px]">
              <span className="flex items-center gap-2 font-semibold">
                <Download size={13} style={{ color: "var(--accent)" }} />
                Подготовка · {PREP_LABELS[prep.phase] ?? prep.phase}
              </span>
              <span className="font-mono-console" style={{ color: "var(--accent)" }}>
                {prep.total ? Math.min(99, Math.round((prep.done / prep.total) * 100)) : 0}%
              </span>
            </div>
            <div className="gauge">
              <div style={{ width: `${prep.total ? Math.max(3, (prep.done / prep.total) * 100) : 5}%` }} />
            </div>
            <div className="mt-1 flex items-center justify-between text-[11px] opacity-50">
              <span>
                {prep.done.toLocaleString("ru")} / {prep.total.toLocaleString("ru")} · кэш: {sys?.dataDir ?? "…"}
              </span>
              <button
                className="btn btn-danger !py-0.5 !px-2 text-[11px]"
                onClick={() => void api.cancelDownload().catch((e) => toast(String(e), "err"))}
              >
                отменить
              </button>
            </div>
          </div>
        )}

        <div className="flex items-center gap-2 text-[12.5px] opacity-60">
          <Coffee size={14} style={{ color: "var(--accent)" }} />
          RAM {ramFmt(settings.minMem)}–{ramFmt(settings.maxMem)} · {JVM_PRESETS[settings.jvmPreset].name}
          <span aria-hidden>·</span>
          <button className="btn btn-ghost !py-1 !px-2 text-[12px]" onClick={() => go("console")}>
            <TerminalSquare size={13} /> консоль
          </button>
        </div>

        {/* quick instance switcher */}
        {instances.length > 1 && (
          <div className="flex max-w-2xl flex-wrap justify-center gap-2">
            {instances.map((i) => (
              <button
                key={i.config.id}
                className={`badge cursor-pointer !text-[11px] ${i.config.id === settings.selectedInstance ? "badge-accent" : ""}`}
                onClick={() => patch({ selectedInstance: i.config.id })}
              >
                {i.config.name}
              </button>
            ))}
          </div>
        )}
      </div>

      <div className="grid shrink-0 grid-cols-3 gap-4">
        <StatCard
          icon={<User size={17} />}
          title="Ник в игре"
          body={
            <div className="space-y-1.5">
              <input
                className={`inp font-mono-console !w-full !text-[12px] ${nameOk ? "" : "!border-[color:var(--danger)]"}`}
                maxLength={16}
                value={settings.playerName}
                placeholder="deBangPlayer"
                aria-label="Ник в игре"
                onChange={(e) =>
                  patch({ playerName: e.target.value.replace(/[^A-Za-z0-9_]/g, "") })
                }
              />
              {nameOk ? (
                <div className="text-[11px] opacity-60">
                  {mem ? `${usedPct.toFixed(0)}% RAM занято · ` : ""}3–16 символов: A–Z, 0–9, _
                </div>
              ) : (
                <div className="text-[11px]" style={{ color: "var(--danger)" }}>
                  Ник должен быть 3–16 символов: A–Z, 0–9, _
                </div>
              )}
            </div>
          }
        />
        <StatCard
          icon={<Cpu size={17} />}
          title="Java runtime"
          body={
            <div className="text-[12px] leading-relaxed opacity-75">
              {settings.javaPath ? (
                <>
                  <div className="font-mono-console truncate text-[11px]" style={{ color: "var(--accent)" }}>
                    {settings.javaPath}
                  </div>
                  <button className="btn btn-ghost mt-1 !p-1 text-[11px]" onClick={() => go("settings")}>
                    сменить
                  </button>
                </>
              ) : (
                <button className="btn btn-ghost !p-1 text-[12px]" onClick={() => go("settings")}>
                  настроить → Настройки › Java
                </button>
              )}
            </div>
          }
        />
        <StatCard
          icon={<Coffee size={17} />}
          title="Выделено JVM"
          body={
            mem ? (
              <>
                <div className="gauge mb-2">
                  <div style={{ width: `${Math.min(100, (settings.maxMem / mem.totalMb) * 100)}%` }} />
                </div>
                <div className="text-[12px] opacity-70">
                  max {ramFmt(settings.maxMem)} = {((settings.maxMem / mem.totalMb) * 100).toFixed(0)}% физической RAM
                </div>
              </>
            ) : memFailed ? (
              <div className="text-[12px] opacity-70">Нет данных о памяти</div>
            ) : null
          }
        />
      </div>
    </div>
  );
}

function StatCard({ icon, title, body, loading }: { icon: React.ReactNode; title: string; body?: React.ReactNode; loading?: boolean }) {
  return (
    <div className="glass card-hover p-4">
      <div className="mb-2.5 flex items-center gap-2 text-[12px] font-semibold uppercase tracking-wider opacity-60">
        {icon} {title}
      </div>
      {loading ? <div className="skel h-10" /> : body}
    </div>
  );
}
