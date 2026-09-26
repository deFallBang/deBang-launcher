import { useEffect, useState } from "react";
import { FlaskConical, Gauge, Network, Save, X } from "lucide-react";
import { api, type InstanceInfo, type ProxyConfig, type ProxyKind } from "../lib/api";
import { useApp } from "../state/app";

const KINDS: Array<{ id: ProxyKind; label: string }> = [
  { id: "None", label: "Без прокси" },
  { id: "Socks5", label: "SOCKS5" },
  { id: "Http", label: "HTTP / HTTPS" },
];

const emptyProxy = (): ProxyConfig => ({ kind: "None", host: "", port: 0, login: "", password: "" });

export function InstanceSettings({
  ins,
  onClose,
  onSaved,
}: {
  ins: InstanceInfo;
  onClose: () => void;
  onSaved: () => void;
}) {
  const { settings, assembledArgs, toast } = useApp();
  const [proxy, setProxy] = useState<ProxyConfig>(ins.config.proxy ?? emptyProxy());
  const [autoGc, setAutoGc] = useState(ins.config.autoGc ?? true);
  const [autoMem, setAutoMem] = useState(ins.config.autoMem ?? false);
  const [plan, setPlan] = useState<Awaited<ReturnType<typeof api.instanceLaunchPlan>> | null>(null);
  const [saving, setSaving] = useState(false);

  const cfg = ins.config;
  useEffect(() => {
    let alive = true;
    api
      .instanceLaunchPlan(ins.config.id, {
        javaPath: settings.javaPath,
        playerName: settings.playerName,
        minMemMb: settings.minMem,
        maxMemMb: settings.maxMem,
        jvmArgs: assembledArgs,
      })
      .then((p) => alive && setPlan(p))
      .catch((e) => {
        if (alive) toast(String(e), "err");
      });
    return () => {
      alive = false;
    };
  }, [ins.config.id, settings, assembledArgs]);

  const patch = (p: Partial<ProxyConfig>) => setProxy((v) => ({ ...v, ...p }));

  async function save() {
    if (proxy.kind !== "None" && !proxy.host.trim()) {
      toast("Укажите хост прокси", "err");
      return;
    }
    if (proxy.kind !== "None" && !proxy.port) {
      toast("Укажите порт прокси", "err");
      return;
    }
    setSaving(true);
    try {
      await api.updateInstanceSettings(cfg.id, {
        proxy: { ...proxy, port: Number(proxy.port) || 0 },
        autoGc,
        autoMem,
      });
      toast(`Настройки профиля «${cfg.name}» сохранены`);
      onSaved();
      onClose();
    } catch (e) {
      toast(String(e), "err");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/50 p-6" onClick={onClose}>
      <div
        className="glass-strong w-full max-w-lg space-y-4 p-5 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2">
          <Gauge size={16} style={{ color: "var(--accent)" }} />
          <h3 className="text-[15px] font-bold">Настройки профиля · {cfg.name}</h3>
          <span className="flex-1" />
          <button className="btn !p-1.5" onClick={onClose} aria-label="Закрыть">
            <X size={14} />
          </button>
        </div>
        <p className="text-[11.5px] opacity-55">
          MC {cfg.version} · {cfg.loader} · применяется только к этой версии
        </p>

        <label className="flex cursor-pointer items-start gap-2.5 rounded-xl border border-white/10 p-3 hover:bg-white/5">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={autoGc}
            onChange={(e) => setAutoGc(e.target.checked)}
          />
          <span>
            <span className="block text-[12.5px] font-semibold">Умный подбор GC</span>
            <span className="block text-[11px] opacity-60">
              Generational ZGC для Java 21+ и MC 1.20.5+, G1GC с ParallelRefProc для старых версий.
              Свой выбор сборщика в настройках JVM имеет приоритет.
            </span>
          </span>
        </label>

        <label className="flex cursor-pointer items-start gap-2.5 rounded-xl border border-white/10 p-3 hover:bg-white/5">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={autoMem}
            onChange={(e) => setAutoMem(e.target.checked)}
          />
          <span>
            <span className="block text-[12.5px] font-semibold">Автовыделение памяти</span>
            <span className="block text-[11px] opacity-60">
              3 ГБ для версий до 1.12.2, 6 ГБ для 1.12.2–1.19, 8 ГБ для 1.20+, но не больше 70% ОЗУ.
            </span>
          </span>
        </label>

        <div className="space-y-2.5 rounded-xl border border-white/10 p-3">
          <div className="flex items-center gap-2 text-[12.5px] font-semibold">
            <Network size={14} style={{ color: "var(--accent)" }} /> Прокси версии
          </div>
          <div className="flex gap-2">
            <select
              className="inp !w-44"
              value={proxy.kind}
              onChange={(e) => patch({ kind: e.target.value as ProxyKind })}
            >
              {KINDS.map((k) => (
                <option key={k.id} value={k.id}>
                  {k.label}
                </option>
              ))}
            </select>
            <input
              className="inp font-mono-console flex-1 !text-[12px]"
              placeholder="хост: 127.0.0.1"
              value={proxy.host}
              disabled={proxy.kind === "None"}
              onChange={(e) => patch({ host: e.target.value.trim() })}
            />
            <input
              className="inp !w-24 font-mono-console !text-[12px]"
              placeholder="порт"
              inputMode="numeric"
              value={proxy.port || ""}
              disabled={proxy.kind === "None"}
              onChange={(e) =>
                patch({
                  port: Math.min(65535, Number(e.target.value.replace(/\D/g, "")) || 0),
                })
              }
            />
          </div>
          {proxy.kind !== "None" && (
            <div className="flex gap-2">
              <input
                className="inp flex-1 !text-[12px]"
                placeholder="логин (необязательно)"
                value={proxy.login}
                onChange={(e) => patch({ login: e.target.value })}
              />
              <input
                className="inp flex-1 !text-[12px]"
                type="password"
                placeholder="пароль (необязательно)"
                value={proxy.password}
                onChange={(e) => patch({ password: e.target.value })}
              />
            </div>
          )}
          <p className="text-[10.5px] opacity-55">
            SOCKS5 → <code>-DsocksProxyHost/-DsocksProxyPort</code>, HTTP →{" "}
            <code>-Dhttp.proxy*</code> и <code>-Dhttps.proxy*</code>. Пароль хранится в
            instance.json (права 600) и не пишется в консоль лаунчера.
          </p>
        </div>

        {plan && (
          <div className="rounded-xl border border-white/10 p-3 text-[11.5px]">
            <div className="mb-1 flex items-center gap-2 font-semibold">
              <FlaskConical size={13} style={{ color: "var(--accent)" }} /> Будет применено при
              запуске
            </div>
            <div className="font-mono-console leading-relaxed opacity-80">
              -Xms{plan.minMemMb}M -Xmx{plan.maxMemMb}M
              {plan.gcFlags.length ? ` ${plan.gcFlags.join(" ")}` : ""}
              {plan.proxyEnabled ? " + прокси" : ""}
            </div>
            {plan.notes.length > 0 && (
              <ul className="mt-1.5 list-disc space-y-0.5 pl-4 opacity-60">
                {plan.notes.map((n) => (
                  <li key={n}>{n}</li>
                ))}
              </ul>
            )}
          </div>
        )}

        <div className="flex justify-end gap-2">
          <button className="btn" onClick={onClose}>
            Отмена
          </button>
          <button className="btn btn-primary" onClick={() => void save()} disabled={saving}>
            {saving ? <span className="spinner !size-3.5" /> : <Save size={14} />} Сохранить
          </button>
        </div>
      </div>
    </div>
  );
}
