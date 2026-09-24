import { useEffect, useState } from "react";
import { Check, Coffee, Cpu, Image, Info, Palette, RotateCcw, Search, SlidersHorizontal, User } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type JavaInstall } from "../lib/api";
import { isValidPlayerName, JVM_PRESETS, useApp } from "../state/app";
import { Slider } from "../components/ui";

const ACCENTS = ["#cba6f7", "#89b4fa", "#a6e3a1", "#fab387", "#f5c2e7", "#88c0d0", "#38bdf8", "#f706cf", "#00f0ff", "#fbbf24"];
type Tab = "appearance" | "java" | "jvm";

export function Settings() {
  const { settings, patch, presets, mem, toast, assembledArgs } = useApp();
  const [tab, setTab] = useState<Tab>("appearance");
  const [java, setJava] = useState<JavaInstall[] | null>(null);
  const [manualPath, setManualPath] = useState("");
  const [manualOk, setManualOk] = useState<string | null>(null);

  useEffect(() => {
    if (tab === "java" && !java) {
      api.detectJava().then(setJava).catch(() => setJava([]));
    }
  }, [tab, java]);

  const maxCap = mem ? Math.max(512, mem.totalMb - 1536) : 16384;
  const nameOk = isValidPlayerName(settings.playerName);

  return (
    <div className="view-enter flex h-full flex-col gap-4 overflow-y-auto p-6 pt-3">
      <div className="flex items-center gap-2">
        <h2 className="text-2xl font-bold">Настройки</h2>
        <div className="flex-1" />
        {([
          { id: "appearance", label: "Оформление", icon: Palette },
          { id: "java", label: "Java", icon: Coffee },
          { id: "jvm", label: "Память и JVM", icon: SlidersHorizontal },
        ] as const).map((t) => (
          <button key={t.id} className={`btn ${tab === t.id ? "btn-primary" : ""}`} onClick={() => setTab(t.id)}>
            <t.icon size={14} /> {t.label}
          </button>
        ))}
      </div>

      {tab === "appearance" && (
        <div className="grid grid-cols-2 gap-4">
          <section className="glass card-hover space-y-4 p-5">
            <SectionTitle icon={<Palette size={15} />} title="Пресеты тем" />
            <div className="grid grid-cols-2 gap-2.5">
              {presets.map((p) => (
                <button
                  key={p.id}
                  className={`glass flex items-center gap-2.5 p-3 text-left !rounded-[calc(var(--radius)*0.6)] transition-transform hover:-translate-y-0.5 ${
                    settings.preset === p.id ? "!border-[color:var(--accent)]" : ""
                  }`}
                  onClick={() => patch({ preset: p.id, accent: null })}
                >
                  <span className="flex gap-1">
                    <i className="dot !bg-[var(--p1)]" style={{ ["--p1" as never]: p.vars["--bg2"] }} />
                    <i className="dot" style={{ background: p.vars["--accent"] }} />
                    <i className="dot" style={{ background: p.vars["--text"] }} />
                  </span>
                  <span className="min-w-0 flex-1 truncate text-[12.5px] font-semibold">{p.name}</span>
                  {settings.preset === p.id && <Check size={14} style={{ color: "var(--accent)" }} />}
                </button>
              ))}
            </div>
            <SectionTitle icon={<Info size={15} />} title="Акцентный цвет" />
            <div className="flex flex-wrap items-center gap-2">
              {ACCENTS.map((c) => (
                <button
                  key={c}
                  className={`dot size-7 !rounded-lg transition-transform hover:scale-110 ${settings.accent === c ? "ring-2 ring-offset-2" : ""}`}
                  style={{ background: c, ["--tw-ring-color" as never]: "var(--text)", ["--tw-ring-offset-color" as never]: "var(--bg2)" }}
                  onClick={() => patch({ accent: settings.accent === c ? null : c })}
                />
              ))}
              <input
                type="color"
                className="swatch"
                value={settings.accent ?? presets.find((p) => p.id === settings.preset)?.vars["--accent"] ?? "#cba6f7"}
                onChange={(e) => patch({ accent: e.target.value })}
              />
              {settings.accent && (
                <button className="btn btn-ghost !py-1 text-[12px]" onClick={() => patch({ accent: null })}>
                  <RotateCcw size={12} /> сбросить
                </button>
              )}
            </div>
            <Slider
              label="Радиус скруглений (карточки, кнопки)"
              value={settings.radius}
              min={0}
              max={28}
              unit="px"
              onChange={(radius) => patch({ radius })}
            />
          </section>

          <section className="glass card-hover space-y-4 p-5">
            <SectionTitle icon={<Image size={15} />} title="Фоновый движок" />
            <div className="flex gap-1">
              {([
                { id: "gradient", label: "Градиент" },
                { id: "image", label: "Фото" },
                { id: "video", label: "Видео" },
              ] as const).map((b) => (
                <button key={b.id} className={`btn flex-1 ${settings.bgType === b.id ? "btn-primary" : ""}`} onClick={() => patch({ bgType: b.id })}>
                  {b.label}
                </button>
              ))}
            </div>
            {settings.bgType !== "gradient" && (
              <>
                <div className="font-mono-console truncate rounded-lg border p-2 text-[11px] opacity-75" style={{ borderColor: "var(--border)" }}>
                  {settings.bgPath || "файл не выбран"}
                </div>
                <button
                  className="btn"
                  onClick={async () => {
                    try {
                      const p = await open({
                        multiple: false,
                        filters:
                          settings.bgType === "video"
                            ? [{ name: "Video", extensions: ["mp4", "webm", "mkv"] }]
                            : [{ name: "Image", extensions: ["png", "jpg", "jpeg", "webp", "gif"] }],
                      });
                      if (!p) return;
                      // копируем в каталог данных лаунчера: asset-scope больше
                      // не открывает вебвью всю домашнюю папку
                      const stored = await api.importBackground(String(p));
                      patch({ bgPath: stored });
                      toast("Фон сохранён в каталог лаунчера");
                    } catch (e) {
                      toast(String(e), "err");
                    }
                  }}
                >
                  Выбрать файл…
                </button>
                <Slider label="Размытие фона" value={settings.blur} min={0} max={40} unit="px" onChange={(blur) => patch({ blur })} />
                <Slider label="Затемнение (dimming)" value={settings.dim} min={0} max={90} unit="%" onChange={(dim) => patch({ dim })} />
                <button className="btn btn-ghost text-[12px]" onClick={() => patch({ bgPath: "" })}>
                  <RotateCcw size={12} /> убрать медиа
                </button>
              </>
            )}
            {settings.bgType === "gradient" && (
              <p className="text-[12px] opacity-50">
                Нео-минимализм: анимированный aurora-градиент из акцентного цвета текущей темы.
              </p>
            )}
          </section>
        </div>
      )}

      {tab === "java" && (
        <div className="space-y-4">
          <section className="glass card-hover space-y-3 p-5">
            <div className="flex items-center">
              <SectionTitle icon={<Cpu size={15} />} title="Автодетекция: archlinux-java · /usr/lib/jvm · $PATH" />
              <button className="btn btn-ghost ml-auto !py-1 text-[12px]" onClick={() => { setJava(null); }}>
                <RotateCcw size={12} /> пересканировать
              </button>
            </div>
            {!java ? (
              <div className="skel h-20" />
            ) : java.length === 0 ? (
              <p className="text-[13px] opacity-60">Java не найдена. Установите: sudo pacman -S jre-openjdk</p>
            ) : (
              <div className="grid grid-cols-2 gap-2.5">
                {java.map((j) => (
                  <button
                    key={j.path}
                    className={`glass flex items-center gap-3 p-3 text-left !rounded-[calc(var(--radius)*0.6)] transition-transform hover:-translate-y-0.5 ${
                      settings.javaPath === j.path ? "!border-[color:var(--accent)]" : ""
                    }`}
                    onClick={() => {
                      patch({ javaPath: j.path });
                      toast(`Java ${j.major} выбрана`);
                    }}
                  >
                    <span className="grid size-10 shrink-0 place-items-center rounded-xl font-black" style={{ background: "color-mix(in oklab, var(--accent) 16%, transparent)", color: "var(--accent)" }}>
                      {j.major}
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-mono-console text-[11px]">{j.path}</span>
                      <span className="block text-[11px] opacity-50">
                        {j.version} · {j.source}
                      </span>
                    </span>
                    {settings.javaPath === j.path && <Check size={15} style={{ color: "var(--accent)" }} />}
                  </button>
                ))}
              </div>
            )}
          </section>

          <section className="glass card-hover space-y-3 p-5">
            <SectionTitle icon={<User size={15} />} title="Ник в игре" />
            <div className="flex items-center gap-2">
              <input
                className={`inp max-w-64 ${nameOk ? "" : "!border-[color:var(--danger)]"}`}
                maxLength={16}
                value={settings.playerName}
                onChange={(e) => patch({ playerName: e.target.value.replace(/[^A-Za-z0-9_]/g, "") })}
                placeholder="deBangPlayer"
                aria-label="Ник в игре"
              />
              {nameOk ? (
                <span className="text-[11.5px] opacity-50">3–16 символов: [A-Za-z0-9_] · подставляется в --username</span>
              ) : (
                <span className="text-[11.5px]" style={{ color: "var(--danger)" }}>
                  Ник должен быть 3–16 символов: A–Z, 0–9, _
                </span>
              )}
            </div>
          </section>

          <section className="glass card-hover space-y-3 p-5">
            <SectionTitle icon={<Search size={15} />} title="Произвольный путь к java" />
            <div className="flex gap-2">
              <input
                className="inp font-mono-console !text-[12px]"
                placeholder="/usr/lib/jvm/java-21-openjdk/bin/java или /opt/graalvm/bin/java"
                value={manualPath}
                onChange={(e) => {
                  setManualPath(e.target.value);
                  setManualOk(null);
                }}
              />
              <button
                className="btn"
                onClick={async () => {
                  setManualOk(null);
                  try {
                    const v = await api.checkJava(manualPath);
                    setManualOk(`OK · Java ${v}`);
                  } catch (e) {
                    setManualOk(`ERR · ${e}`);
                  }
                }}
              >
                Проверить
              </button>
              <button
                className="btn btn-primary"
                disabled={!manualOk?.startsWith("OK")}
                onClick={() => {
                  patch({ javaPath: manualPath });
                  toast("Путь сохранён");
                }}
              >
                Использовать
              </button>
            </div>
            {manualOk && (
              <p className={`font-mono-console text-[12px] ${manualOk.startsWith("OK") ? "" : "text-red-400"}`} style={manualOk.startsWith("OK") ? { color: "var(--good)" } : undefined}>
                {manualOk}
              </p>
            )}
          </section>
        </div>
      )}

      {tab === "jvm" && (
        <div className="grid grid-cols-2 gap-4">
          <section className="glass card-hover space-y-4 p-5">
            <SectionTitle icon={<Coffee size={15} />} title="Память (из /proc/meminfo)" />
            {mem ? (
              <div className="text-[12.5px] opacity-70">
                MemTotal: <b style={{ color: "var(--text)" }}>{(mem.totalMb / 1024).toFixed(1)} GiB</b> · MemAvailable:{" "}
                <b style={{ color: "var(--good)" }}>{(mem.availableMb / 1024).toFixed(1)} GiB</b>
              </div>
            ) : (
              <div className="skel h-6" />
            )}
            <Slider
              label="Минимум RAM (-Xms)"
              value={settings.minMem}
              min={512}
              max={maxCap}
              step={256}
              unit=" MB"
              onChange={(v) => patch({ minMem: Math.min(v, settings.maxMem) })}
            />
            <Slider
              label="Максимум RAM (-Xmx)"
              value={settings.maxMem}
              min={512}
              max={maxCap}
              step={256}
              unit=" MB"
              hint={`защита: не больше ${(maxCap / 1024).toFixed(1)} GiB из ${(mem ? mem.totalMb / 1024 : 0).toFixed(1)} GiB системы`}
              onChange={(v) => patch({ maxMem: Math.max(v, settings.minMem) })}
            />
          </section>

          <section className="glass card-hover space-y-4 p-5">
            <SectionTitle icon={<SlidersHorizontal size={15} />} title="Пресеты тюнинга JVM" />
            <div className="grid grid-cols-2 gap-2">
              {(Object.keys(JVM_PRESETS) as Array<keyof typeof JVM_PRESETS>).map((k) => (
                <button key={k} className={`btn ${settings.jvmPreset === k ? "btn-primary" : ""}`} onClick={() => patch({ jvmPreset: k })}>
                  {JVM_PRESETS[k].name}
                </button>
              ))}
            </div>
            <div>
              <label className="mb-1.5 block text-[12px] opacity-70">Пользовательские Java-аргументы</label>
              <textarea
                className="inp font-mono-console resize-none !text-[11.5px]"
                rows={3}
                placeholder="-XX:+UseNUMA -Dsun.management.compiler=2"
                value={settings.customArgs}
                onChange={(e) => patch({ customArgs: e.target.value })}
              />
            </div>
            <div>
              <div className="mb-1.5 text-[12px] opacity-70">Итоговые флаги запуска</div>
              <div className="glass-strong max-h-32 overflow-y-auto p-3 font-mono-console text-[11px] leading-relaxed" style={{ color: "var(--text-dim)" }}>
                -Xms{settings.minMem}M -Xmx{settings.maxMem}M {assembledArgs.join(" ")}
              </div>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

function SectionTitle({ icon, title }: { icon: React.ReactNode; title: string }) {
  return (
    <div className="flex items-center gap-2 text-[13px] font-bold uppercase tracking-wider opacity-75">
      <span style={{ color: "var(--accent)" }}>{icon}</span>
      {title}
    </div>
  );
}
