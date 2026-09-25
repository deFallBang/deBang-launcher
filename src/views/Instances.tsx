import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, Boxes, Check, Download, FolderOpen, Network, PackageOpen, Plus, Settings2, Trash2, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api, type McVersion } from "../lib/api";
import { useApp } from "../state/app";
import { Skeleton } from "../components/ui";
import { InstanceSettings } from "../components/InstanceSettings";

const LOADERS = ["Vanilla", "Fabric", "Forge", "NeoForge"] as const;

export function Instances() {
  const { instances, instancesFailed, refreshInstances, settings, patch, toast, prep, status, sys } = useApp();
  const [versions, setVersions] = useState<McVersion[] | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [name, setName] = useState("");
  const [ver, setVer] = useState("1.21.8");
  const [loader, setLoader] = useState<(typeof LOADERS)[number]>("Vanilla");
  const [creating, setCreating] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [versionsError, setVersionsError] = useState(false);

  useEffect(() => {
    let disposed = false;
    api
      .mojangVersions()
      .then((v) => {
        if (!disposed) {
          setVersions(v);
          setVersionsError(false);
        }
      })
      .catch(() => {
        if (!disposed) setVersionsError(true);
      });
    return () => {
      disposed = true;
    };
  }, []);

  const releases = useMemo(
    () => (versions ?? []).filter((v) => v.type === "release").slice(0, 40),
    [versions],
  );

  async function create() {
    if (!name.trim()) return;
    setCreating(true);
    try {
      await api.createInstance(name.trim(), ver, loader);
      toast(`Инстанс «${name}» создан`);
      setShowCreate(false);
      setName("");
      await refreshInstances();
    } catch (e) {
      toast(String(e), "err");
    } finally {
      setCreating(false);
    }
  }

  async function remove(id: string, name: string) {
    if (!window.confirm(`Удалить инстанс «${name}»? Мир, моды и настройки будут стёрты безвозвратно.`)) {
      return;
    }
    if (status?.running && status.instanceId === id) {
      toast("Сначала остановите игру", "err");
      return;
    }
    setBusyId(id);
    try {
      await api.deleteInstance(id);
      toast("Инстанс удалён");
      await refreshInstances();
    } catch (e) {
      toast(String(e), "err");
    } finally {
      setBusyId(null);
    }
  }

  async function installMrpack() {
    try {
      const p = await open({
        multiple: false,
        title: "Выберите файл сборки .mrpack",
        filters: [{ name: "Modpack", extensions: ["mrpack"] }],
      });
      if (!p) return;
      const file = String(p);
      const fn = file.split("/").pop() ?? "pack.mrpack";
      toast("Устанавливаю сборку…");
      const id = await api.installModpack(file, fn, fn.replace(/\.mrpack$/i, ""));
      await refreshInstances();
      patch({ selectedInstance: id });
      toast(`Сборка установлена → инстанс «${id}»`);
    } catch (e) {
      toast(String(e), "err");
    }
  }

  async function importRun(id: string) {
    try {
      const path = await open({
        multiple: false,
        title: "Выберите minecraft.jar, server.jar или run.sh",
        filters: [{ name: "Run", extensions: ["jar", "sh"] }],
      });
      if (!path) return;
      await api.importRunFile(id, String(path));
      toast("Файл импортирован — запуск станет реальным");
      await refreshInstances();
    } catch (e) {
      toast(String(e), "err");
    }
  }

  return (
    <div className="view-enter flex h-full flex-col gap-4 overflow-y-auto p-6 pt-3">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold">Инстансы</h2>
          <p className="text-[12.5px] opacity-55">
            Изолированные сборки в {sys ? `${sys.dataDir}/instances` : "каталоге данных лаунчера"}
          </p>
        </div>
        <button className="btn" onClick={() => void installMrpack()}>
          <PackageOpen size={15} /> Установить .mrpack
        </button>
        <button className="btn btn-primary" onClick={() => setShowCreate(true)}>
          <Plus size={15} /> Новый инстанс
        </button>
      </div>

      {prep && (prep.phase === "modpack" || prep.phase === "mrpack") && (
        <div className="glass px-4 py-3">
          <div className="mb-1.5 flex justify-between text-[11.5px]">
            <span className="font-semibold" style={{ color: "var(--accent)" }}>
              Установка сборки…
            </span>
            <span className="flex items-center gap-2">
              <span className="font-mono-console opacity-60">
                {prep.done}/{prep.total} файлов
              </span>
              <button
                className="btn btn-danger !py-0.5 !px-2 text-[11px]"
                onClick={() => void api.cancelDownload().catch((e) => toast(String(e), "err"))}
              >
                отменить
              </button>
            </span>
          </div>
          <div className="gauge">
            <div style={{ width: `${prep.total ? Math.max(3, (prep.done / prep.total) * 100) : 5}%` }} />
          </div>
        </div>
      )}

      {showCreate && (
        <div className="glass-strong flex flex-wrap items-end gap-4 p-5">
          <div className="min-w-52 flex-1">
            <label className="mb-1 block text-[12px] opacity-70">Название</label>
            <input
              autoFocus
              className="inp"
              placeholder="Напр. Vanilla Survival"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </div>
          <div className="min-w-40">
            <label className="mb-1 block text-[12px] opacity-70">Версия игры</label>
            <div className="relative">
              <select className="inp" value={ver} onChange={(e) => setVer(e.target.value)}>
                {!versions ? (
                  <option>загрузка…</option>
                ) : (
                  releases.map((v) => (
                    <option key={v.id} value={v.id}>
                      {v.id} · {v.releaseTime.slice(0, 10)}
                    </option>
                  ))
                )}
              </select>
            </div>
          </div>
          <div>
            <label className="mb-1 block text-[12px] opacity-70">Модлоадер</label>
            <div className="flex gap-1">
              {LOADERS.map((l) => (
                <button
                  key={l}
                  className={`btn !py-1.5 !px-3 ${loader === l ? "btn-primary" : ""}`}
                  onClick={() => setLoader(l)}
                >
                  {l}
                </button>
              ))}
            </div>
          </div>
          <button className="btn btn-primary" onClick={create} disabled={creating || !name.trim()}>
            {creating ? <span className="spinner !size-4" /> : <Check size={15} />} Создать
          </button>
          <button className="btn btn-ghost" onClick={() => setShowCreate(false)}>
            <X size={15} />
          </button>
        </div>
      )}

      {instancesFailed ? (
        <div className="glass flex items-center gap-3 p-6 text-[13px]">
          <AlertTriangle size={18} style={{ color: "var(--danger)" }} />
          Не удалось прочитать список инстансов.
          <button className="btn !py-1" onClick={() => void refreshInstances()}>
            Повторить
          </button>
        </div>
      ) : !versions && !versionsError ? (
        <div className="grid grid-cols-2 gap-4 xl:grid-cols-3">
          {[0, 1, 2].map((i) => (
            <Skeleton key={i} className="h-40" />
          ))}
        </div>
      ) : versionsError ? (
        <div className="glass grid place-items-center gap-2 p-12 text-center">
          <AlertTriangle size={26} style={{ color: "var(--danger)" }} />
          <p className="text-[13px]">Список версий Mojang недоступен — проверьте интернет.</p>
          <button className="btn" onClick={() => window.location.reload()}>
            Обновить
          </button>
        </div>
      ) : instances.length === 0 ? (
        <div className="glass grid place-items-center gap-3 p-16 text-center opacity-80">
          <Boxes size={40} style={{ color: "var(--accent)" }} />
          <p className="text-[14px]">
            Пока пусто. Создайте инстанс — каждый профиль получает изолированные
            <br />
            mods / resourcepacks / shaderpacks / saves.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-4 xl:grid-cols-3">
          {instances.map((ins) => {
            const active = ins.config.id === settings.selectedInstance;
            return (
              <div
                key={ins.config.id}
                className={`glass card-hover relative flex flex-col gap-3 p-4 ${active ? "!border-[color:var(--accent)]" : ""}`}
              >
                <div className="flex items-start justify-between">
                  <div>
                    <div className="text-[15px] font-bold">{ins.config.name}</div>
                    <div className="mt-1.5 flex gap-1.5">
                      <span className="badge badge-accent">{ins.config.version}</span>
                      <span className="badge">{ins.config.loader}</span>
                    </div>
                  </div>
                  {active && (
                    <span className="badge badge-accent flex items-center gap-1">
                      <Check size={11} /> активный
                    </span>
                  )}
                </div>
                <div className="flex flex-wrap items-center gap-1.5 text-[11.5px] opacity-55">
                  <span>
                    {ins.modCount} модов · {ins.hasRunScript ? "есть run.sh" : "авто-бутстрап"}
                  </span>
                  {ins.config.proxy && ins.config.proxy.kind !== "None" && (
                    <span className="badge badge-accent flex items-center gap-1 !py-0 !text-[10px]">
                      <Network size={9} /> {ins.config.proxy.kind.toLowerCase()}
                    </span>
                  )}
                  {ins.config.autoGc && <span className="badge !py-0 !text-[10px]">авто-GC</span>}
                  {ins.config.autoMem && <span className="badge !py-0 !text-[10px]">авто-RAM</span>}
                </div>
                <div className="font-mono-console truncate text-[10.5px] opacity-40">{ins.dir}</div>
                <div className="mt-auto flex gap-2">
                  <button
                    className={`btn flex-1 ${active ? "btn-primary" : ""}`}
                    onClick={() => patch({ selectedInstance: ins.config.id })}
                  >
                    {active ? "Выбран" : "Выбрать"}
                  </button>
                  <button
                    className="btn"
                    title="Настройки профиля: авто-GC, авто-память, прокси"
                    aria-label={`Настройки профиля ${ins.config.name}`}
                    onClick={() => setEditing(ins.config.id)}
                  >
                    <Settings2 size={14} />
                  </button>
                  <button
                    className="btn"
                    title="Импортировать run.sh / jar в инстанс"
                    aria-label={`Импортировать run.sh в ${ins.config.name}`}
                    disabled={busyId === ins.config.id}
                    onClick={() => void importRun(ins.config.id)}
                  >
                    {busyId === ins.config.id ? <span className="spinner !size-3.5" /> : <Download size={14} />}
                  </button>
                  <button
                    className="btn btn-danger"
                    title="Удалить"
                    aria-label={`Удалить инстанс ${ins.config.name}`}
                    disabled={busyId === ins.config.id || (status?.running && status.instanceId === ins.config.id)}
                    onClick={() => void remove(ins.config.id, ins.config.name)}
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
      {editing &&
        (() => {
          const target = instances.find((i) => i.config.id === editing);
          return target ? (
            <InstanceSettings
              ins={target}
              onClose={() => setEditing(null)}
              onSaved={() => void refreshInstances()}
            />
          ) : null;
        })()}

      <p className="flex items-center gap-1.5 text-[11px] opacity-40">
        <FolderOpen size={12} /> Vanilla, Fabric и NeoForge скачиваются и запускаются автоматически; для классического Forge — импорт run.sh.
      </p>
    </div>
  );
}
