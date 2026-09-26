import { useCallback, useEffect, useMemo, useState } from "react";
import { AlertTriangle, FolderOpen, Package, Power, RefreshCw, Search, Trash2, X } from "lucide-react";
import { api, type InstanceInfo, type ModEntry } from "../lib/api";
import { useApp } from "../state/app";

const humanSize = (bytes: number) => {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} МБ`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} КБ`;
  return `${bytes} Б`;
};

export function InstanceMods({
  ins,
  onClose,
  onChanged,
}: {
  ins: InstanceInfo;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { toast } = useApp();
  const id = ins.config.id;
  const [mods, setMods] = useState<ModEntry[] | null>(null);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<ModEntry | null>(null);
  const [onlyEnabled, setOnlyEnabled] = useState(false);

  const load = useCallback(() => {
    api
      .listInstanceMods(id)
      .then(setMods)
      .catch((e) => {
        setMods([]);
        toast(String(e), "err");
      });
  }, [id, toast]);

  useEffect(() => {
    load();
  }, [load]);

  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    return (mods ?? []).filter(
      (m) => (!onlyEnabled || m.enabled) && (!q || m.name.toLowerCase().includes(q)),
    );
  }, [mods, query, onlyEnabled]);

  const enabled = (mods ?? []).filter((m) => m.enabled).length;
  const disabled = (mods ?? []).length - enabled;

  async function toggle(mod: ModEntry) {
    setBusy(mod.file);
    try {
      await api.toggleInstanceMod(id, mod.file, !mod.enabled);
      toast(
        mod.enabled ? `Мод «${mod.name}» выключен` : `Мод «${mod.name}» включён`,
        mod.enabled ? undefined : "ok",
      );
      load();
      onChanged();
    } catch (e) {
      toast(String(e), "err");
    } finally {
      setBusy(null);
    }
  }

  async function remove(mod: ModEntry) {
    setBusy(mod.file);
    try {
      await api.deleteInstanceMod(id, mod.file);
      toast(`Мод «${mod.name}» удалён`);
      setConfirm(null);
      load();
      onChanged();
    } catch (e) {
      toast(String(e), "err");
    } finally {
      setBusy(null);
    }
  }

  async function openFolder() {
    try {
      await api.openInstanceFolder(id);
    } catch (e) {
      toast(String(e), "err");
    }
  }

  return (
    <div className="fixed inset-0 z-40 grid place-items-center bg-black/50 p-6" onClick={onClose}>
      <div
        className="glass-strong flex max-h-[80vh] w-full max-w-2xl flex-col gap-3 p-5 shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2">
          <Package size={16} style={{ color: "var(--accent)" }} />
          <h3 className="text-[15px] font-bold">Моды · {ins.config.name}</h3>
          <span className="flex-1" />
          <button className="btn !p-1.5" onClick={load} aria-label="Обновить список" title="Обновить">
            <RefreshCw size={14} />
          </button>
          <button className="btn !p-1.5" onClick={() => void openFolder()} aria-label="Открыть папку" title="Открыть папку версии">
            <FolderOpen size={14} />
          </button>
          <button className="btn !p-1.5" onClick={onClose} aria-label="Закрыть">
            <X size={14} />
          </button>
        </div>

        <div className="flex flex-wrap items-center gap-2 text-[11.5px] opacity-60">
          <span className="badge badge-accent">{enabled} включено</span>
          {disabled > 0 && <span className="badge">{disabled} выключено</span>}
          <span>выключенный мод просто не подхватывается игрой</span>
        </div>

        <div className="flex gap-2">
          <div className="inp flex flex-1 items-center gap-2 !py-1.5">
            <Search size={13} className="opacity-50" />
            <input
              className="w-full bg-transparent text-[12.5px] outline-none"
              placeholder="Поиск по названию"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
          </div>
          <label className="flex cursor-pointer items-center gap-1.5 whitespace-nowrap text-[12px]">
            <input
              type="checkbox"
              checked={onlyEnabled}
              onChange={(e) => setOnlyEnabled(e.target.checked)}
            />
            только включённые
          </label>
        </div>

        <div className="min-h-[120px] flex-1 space-y-1.5 overflow-y-auto pr-1">
          {mods === null ? (
            <div className="grid place-items-center py-10 opacity-60">
              <span className="spinner" /> Загрузка…
            </div>
          ) : mods.length === 0 ? (
            <p className="py-10 text-center text-[12.5px] opacity-60">
              В этой версии нет модов. Файл .jar в папке mods — и он появится здесь.
            </p>
          ) : shown.length === 0 ? (
            <p className="py-10 text-center text-[12.5px] opacity-60">Ничего не найдено</p>
          ) : (
            shown.map((m) => (
              <div
                key={m.file}
                className={`flex items-center gap-2 rounded-xl border border-white/10 px-3 py-2 ${
                  m.enabled ? "" : "opacity-55"
                }`}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5 text-[12.5px] font-semibold">
                    <span className="truncate">{m.name}</span>
                    {m.broken && (
                      <span
                        className="badge badge-accent flex items-center gap-1 !py-0 !text-[10px]"
                        title="Файл не читается как архив — похоже на повреждённую загрузку"
                      >
                        <AlertTriangle size={9} /> битый
                      </span>
                    )}
                  </div>
                  <div className="truncate font-mono-console text-[10.5px] opacity-45">
                    {m.file} · {humanSize(m.size)}
                  </div>
                </div>
                <button
                  className={`btn ${m.enabled ? "" : "btn-primary"}`}
                  title={m.enabled ? "Выключить мод" : "Включить мод"}
                  aria-label={`${m.enabled ? "Выключить" : "Включить"} мод ${m.name}`}
                  disabled={busy === m.file}
                  onClick={() => void toggle(m)}
                >
                  <Power size={14} />
                </button>
                <button
                  className="btn btn-danger"
                  title="Удалить мод"
                  aria-label={`Удалить мод ${m.name}`}
                  disabled={busy === m.file}
                  onClick={() => setConfirm(m)}
                >
                  <Trash2 size={14} />
                </button>
              </div>
            ))
          )}
        </div>

        {confirm && (
          <div className="rounded-xl border border-[color:var(--accent)] p-3">
            <p className="text-[12.5px] font-semibold">Удалить мод «{confirm.name}»?</p>
            <p className="mt-0.5 text-[11.5px] opacity-60">
              Файл <code className="font-mono-console">{confirm.file}</code> будет удалён из папки
              mods без возможности восстановления. Чтобы временно отключить мод — нажмите «Выключить».
            </p>
            <div className="mt-2.5 flex justify-end gap-2">
              <button className="btn" onClick={() => setConfirm(null)}>
                Отмена
              </button>
              <button
                className="btn btn-danger"
                disabled={busy === confirm.file}
                onClick={() => void remove(confirm)}
              >
                {busy === confirm.file ? <span className="spinner !size-3.5" /> : <Trash2 size={14} />}
                Удалить
              </button>
            </div>
          </div>
        )}

        <div className="flex justify-end">
          <button className="btn" onClick={onClose}>
            Готово
          </button>
        </div>
      </div>
    </div>
  );
}
