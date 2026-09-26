import { useCallback, useEffect, useRef, useState } from "react";
import { ArrowDownWideNarrow, Boxes, Check, Download, Flame, Globe2, KeyRound, Layers, Package, Search, Sparkles, X } from "lucide-react";
import { api, type CfFile, type CfProject, type ModrinthHit, type MrVersion } from "../lib/api";
import { useApp } from "../state/app";
import { CardGridSkeleton } from "../components/ui";

const TYPES = [
  { id: "mod", label: "Моды" },
  { id: "resourcepack", label: "Ресурспаки" },
  { id: "shader", label: "Шейдеры" },
  { id: "modpack", label: "Сборки" },
] as const;

const SOURCES = [
  { id: "modrinth", label: "Modrinth" },
  { id: "curseforge", label: "CurseForge" },
] as const;

const CF_CLASSES = [
  { id: 6, label: "Моды" },
  { id: 12, label: "Ресурспаки" },
  { id: 6556, label: "Шейдеры" },
  { id: 4471, label: "Сборки" },
] as const;

const CATS = ["optimization", "technology", "adventure", "decoration", "magic", "tech", "storage", "worldgen", "pvp", "food"];
const SORTS = [
  { index: "relevance", label: "Релевантность" },
  { index: "downloads", label: "Загрузки" },
  { index: "follows", label: "Подписчики" },
  { index: "newest", label: "Новые" },
];

export function Catalog() {
  const { settings, instances, patch, refreshInstances, toast, prep } = useApp();
  const [q, setQ] = useState("");
  const [type, setType] = useState<(typeof TYPES)[number]["id"]>("mod");
  const [cat, setCat] = useState<string | null>(null);
  const [sort, setSort] = useState("relevance");
  const [hits, setHits] = useState<ModrinthHit[] | null>(null);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(false);
  const [dl, setDl] = useState<string | null>(null);
  const [done, setDone] = useState<Set<string>>(new Set());
  const [packVers, setPackVers] = useState<MrVersion[]>([]);
  const [picker, setPicker] = useState<string | null>(null);
  const [source, setSource] = useState<(typeof SOURCES)[number]["id"]>("modrinth");
  const [cfClass, setCfClass] = useState(6);
  const [cfHaveKey, setCfHaveKey] = useState<boolean | null>(null);
  const [cfProjects, setCfProjects] = useState<CfProject[] | null>(null);
  const [cfFiles, setCfFiles] = useState<{ id: number; name: string; files: CfFile[] } | null>(null);
  const reqId = useRef(0);

  const selected = instances.find((i) => i.config.id === settings.selectedInstance) ?? null;

  const search = useCallback(async () => {
    const id = ++reqId.current;
    setLoading(true);
    try {
      // Modrinth quirks (verified against live API):
      //  - facet key is `project_type`, not `type`
      //  - index=relevance + facets => empty result => omit index for relevance
      //  - relevance with empty query => empty result => fall back to downloads
      const facets: string[][] = [[`project_type:${type}`]];
      if (cat) facets[0].push(`categories:${cat}`);
      const hasQ = q.trim().length > 0;
      const index = !hasQ && sort === "relevance" ? "downloads" : sort === "relevance" ? "" : sort;
      const res = await api.modrinthSearch(q.trim(), index, JSON.stringify(facets), 21, 0);
      if (id !== reqId.current) return;
      setHits(res.hits);
      setTotal(res.total_hits ?? res.hits.length);
    } catch (e) {
      if (id !== reqId.current) return;
      toast(String(e), "err");
      setHits([]);
    } finally {
      if (id === reqId.current) setLoading(false);
    }
  }, [q, type, cat, sort, toast]);

  useEffect(() => {
    const t = setTimeout(() => void search(), q ? 450 : 0);
    return () => clearTimeout(t);
  }, [search, q]);

  useEffect(() => {
    if (source !== "curseforge" || cfHaveKey !== null) return;
    api
      .cfKeyStatus()
      .then((st) => setCfHaveKey(st.configured))
      .catch(() => setCfHaveKey(false));
  }, [source, cfHaveKey]);

  const cfSearch = useCallback(async () => {
    if (source !== "curseforge") return;
    const id = ++reqId.current;
    setLoading(true);
    try {
      const res = await api.cfSearch({
        key: "",
        searchFilter: q.trim(),
        classId: cfClass,
        gameVersion: cfClass === 6 ? selected?.config.version ?? null : null,
        page: 0,
      });
      if (id !== reqId.current) return;
      setCfProjects(res.data ?? []);
    } catch (e) {
      if (id !== reqId.current) return;
      toast(String(e), "err");
      setCfProjects([]);
    } finally {
      if (id === reqId.current) setLoading(false);
    }
  }, [source, q, cfClass, selected, toast]);

  useEffect(() => {
    if (source !== "curseforge") return;
    const t = setTimeout(() => void cfSearch(), q ? 450 : 0);
    return () => clearTimeout(t);
  }, [cfSearch, q, source]);

  // установка файла/сборки CurseForge
  const cfInstallFile = useCallback(
    async (p: CfProject, f: CfFile, sub: string) => {
      if (!selected) {
        toast("Выберите версию (вкладка Версии)", "err");
        return;
      }
      if (dl) return;
      setDl(String(p.id));
      try {
        const target = await api.cfDownloadFile({
          key: "",
          instanceId: selected.config.id,
          fileId: f.id,
          filename: f.fileName,
          sub,
        });
        setDone((s) => new Set(s).add(`cf:${p.id}`));
        toast(`Сохранено в ${sub}: ${target.split("/").pop()}`);
      } catch (e) {
        toast(String(e), "err");
      } finally {
        setDl(null);
      }
    },
    [selected, dl, toast],
  );

  const cfInstallModpack = useCallback(
    async (p: CfProject, f: CfFile) => {
      if (dl) return;
      setDl(String(p.id));
      try {
        const id = await api.cfInstallModpack("", f.id, p.name);
        setDone((s) => new Set(s).add(`cf:${p.id}`));
        await refreshInstances();
        patch({ selectedInstance: id });
        toast(`Сборка «${p.name}» установлена — версия «${id}» выбрана`);
      } catch (e) {
        toast(String(e), "err");
      } finally {
        setDl(null);
      }
    },
    [dl, refreshInstances, patch, toast],
  );

  // раскрыть список файлов проекта
  const cfOpenFiles = useCallback(
    async (p: CfProject) => {
      if (cfFiles?.id === p.id) {
        setCfFiles(null);
        return;
      }
      try {
        const res = await api.cfFiles("", p.id);
        setCfFiles({ id: p.id, name: p.name, files: res.data ?? [] });
      } catch (e) {
        toast(String(e), "err");
      }
    },
    [cfFiles, toast],
  );

  async function download(hit: ModrinthHit) {
    setDl(hit.project_id);
    try {
      if (type === "modpack") {
        const all = await api.modrinthVersions(hit.project_id, "", "");
        const opts = all.filter((v) => v.files.some((f) => f.filename.endsWith(".mrpack")));
        if (!opts.length) throw new Error("У сборки нет .mrpack файла");
        if (opts.length === 1) return await installPack(hit, opts[0]);
        setPackVers(opts);
        setPicker(hit.project_id);
        return;
      }
      if (!selected) {
        toast("Выберите версию (вкладка Версии)", "err");
        return;
      }
      const loader = selected.config.loader.toLowerCase();
      const isMod = type === "mod";
      const vers = await api.modrinthVersions(
        hit.project_id,
        isMod && loader !== "vanilla" ? loader : "",
        isMod ? selected.config.version : "",
      );
      const file = vers.find((v) => v.files.length)?.files[0];
      if (!file)
        throw new Error(
          isMod
            ? `Нет файлов под MC ${selected.config.version} / ${selected.config.loader}`
            : "Нет доступных файлов",
        );
      const sub = type === "shader" ? "shaderpacks" : type === "resourcepack" ? "resourcepacks" : "mods";
      const target = await api.downloadMod(selected.config.id, file.url, file.filename, sub);
      setDone((s) => new Set(s).add(hit.project_id));
      toast(`Сохранено в ${sub}: ${target.split("/").pop()}`);
    } catch (e) {
      toast(String(e), "err");
    } finally {
      setDl(null);
    }
  }

  async function installPack(hit: ModrinthHit, v: MrVersion) {
    if (dl) return;
    setPicker(null);
    setDl(hit.project_id);
    try {
      const file = v.files.find((f) => f.filename.endsWith(".mrpack"))!;
      const id = await api.installModpack(file.url, file.filename, hit.title);
      setDone((s) => new Set(s).add(hit.project_id));
      await refreshInstances();
      patch({ selectedInstance: id });
      toast(`Сборка «${v.name}» установлена — версия «${id}» выбрана`);
    } catch (e) {
      toast(String(e), "err");
    } finally {
      setDl(null);
    }
  }

  return (
    <div className="view-enter flex h-full flex-col gap-4 overflow-y-auto p-6 pt-3">
      <div>
        <h2 className="flex items-center gap-2 text-2xl font-bold">
          <Globe2 size={22} style={{ color: "var(--accent)" }} /> Каталог
        </h2>
        <p className="text-[12.5px] opacity-55">
          modrinth.com/v2 · скачанные файлы кладутся в mods активной версии
          {selected ? <b style={{ color: "var(--accent)" }}> «{selected.config.name}»</b> : " (версия не выбрана)"}
        </p>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-64 flex-1">
          <Search size={15} className="absolute left-3 top-1/2 -translate-y-1/2 opacity-50" />
          <input
            className="inp !pl-9"
            placeholder="Поиск: Sodium, Iris, JEI…"
            value={q}
            onChange={(e) => setQ(e.target.value)}
          />
        </div>
        <div className="flex gap-1">
          {SOURCES.map((src) => (
            <button
              key={src.id}
              className={`btn !py-1.5 ${source === src.id ? "btn-primary" : ""}`}
              onClick={() => {
                setSource(src.id);
                setCat(null);
                setHits(source === src.id ? hits : null);
                setCfProjects(null);
                setCfFiles(null);
              }}
            >
              {src.label}
            </button>
          ))}
        </div>
        {source === "modrinth" ? (
          <div className="flex gap-1">
            {TYPES.map((t) => (
              <button key={t.id} className={`btn !py-1.5 ${type === t.id ? "btn-primary" : ""}`} onClick={() => setType(t.id)}>
                {t.label}
              </button>
            ))}
          </div>
        ) : (
          <div className="flex gap-1">
            {CF_CLASSES.map((c) => (
              <button
                key={c.id}
                className={`btn !py-1.5 ${cfClass === c.id ? "btn-primary" : ""}`}
                onClick={() => {
                  setCfClass(c.id);
                  setCfProjects(null);
                  setCfFiles(null);
                }}
              >
                {c.label}
              </button>
            ))}
          </div>
        )}
        <select className="inp !w-auto" value={sort} onChange={(e) => setSort(e.target.value)}>
          {SORTS.map((s) => (
            <option key={s.index} value={s.index}>
              {s.label}
            </option>
          ))}
        </select>
      </div>

      <div className={`flex flex-wrap gap-1.5 ${source === "curseforge" ? "hidden" : ""}`}>
        <button className={`badge cursor-pointer !py-1 ${!cat ? "badge-accent" : ""}`} onClick={() => setCat(null)}>
          все
        </button>
        {CATS.map((c) => (
          <button key={c} className={`badge cursor-pointer !py-1 ${cat === c ? "badge-accent" : ""}`} onClick={() => setCat(cat === c ? null : c)}>
            #{c}
          </button>
        ))}
      </div>

      {prep && prep.phase === "modpack" && (
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
                onClick={() => void api.cancelDownload()}
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

      {source === "curseforge" ? (
        cfHaveKey === false ? (
          <div className="glass grid place-items-center gap-3 p-14 text-center">
            <KeyRound size={30} style={{ color: "var(--accent)" }} />
            <p className="text-[13.5px] font-semibold">Нужен ключ CurseForge API</p>
            <p className="max-w-lg text-[12px] opacity-60">
              CurseForge требует личный ключ: открой curseforge.com/minecraft → Settings → API
              Key, скопируй и вставь в Настройки → CurseForge. Источник Modrinth работает без
              ключа.
            </p>
          </div>
        ) : loading || !cfProjects ? (
          <CardGridSkeleton />
        ) : cfProjects.length === 0 ? (
          <div className="glass grid place-items-center gap-2 p-14 text-center opacity-70">
            <Sparkles size={28} style={{ color: "var(--accent)" }} />
            <p className="text-[13px]">Ничего не найдено — измени запрос или раздел.</p>
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {cfProjects.map((p) => {
              const isPack = cfClass === 4471;
              const opened = cfFiles?.id === p.id;
              return (
                <div key={p.id} className="glass card-hover flex gap-3.5 p-4">
                  {p.logo?.thumbnailUrl ? (
                    <img src={p.logo.thumbnailUrl} alt="" loading="lazy" className="size-14 shrink-0 rounded-xl object-cover" />
                  ) : (
                    <div className="grid size-14 shrink-0 place-items-center rounded-xl" style={{ background: "var(--surface-strong)" }}>
                      {isPack ? <Boxes size={20} style={{ color: "var(--accent)" }} /> : <Flame size={20} style={{ color: "var(--accent)" }} />}
                    </div>
                  )}
                  <div className="min-w-0 flex-1">
                    <div className="truncate text-[14px] font-bold">{p.name}</div>
                    <p className="mt-1 line-clamp-2 text-[11.5px] leading-snug opacity-70">{p.summary}</p>
                    <div className="mt-2 flex items-center gap-2">
                      <span className="flex items-center gap-1 text-[11px] opacity-60">
                        <ArrowDownWideNarrow size={11} style={{ color: "var(--accent)" }} />
                        {Intl.NumberFormat("ru", { notation: "compact" }).format(p.downloadCount)}
                      </span>
                      <span className="flex-1" />
                      <button
                        className="btn !py-1 !px-2.5 text-[12px]"
                        onClick={() => void cfOpenFiles(p)}
                      >
                        {opened ? <X size={13} /> : <Search size={13} />}
                        {isPack ? "файл манифеста" : "файлы"}
                      </button>
                      {isPack && opened && cfFiles?.files[0] && (
                        <button
                          className="btn btn-primary !py-1 !px-2.5 text-[12px]"
                          disabled={dl !== null}
                          onClick={() => void cfInstallModpack(p, cfFiles.files[0])}
                        >
                          {dl === String(p.id) ? <span className="spinner !size-3.5" /> : <Package size={13} />}
                          установить сборку
                        </button>
                      )}
                    </div>
                    {opened && cfFiles && (
                      <div className="mt-2 max-h-56 space-y-1 overflow-y-auto rounded-xl border border-white/10 bg-black/25 p-1.5">
                        {cfFiles.files.length === 0 && (
                          <p className="px-2 py-1 text-[11.5px] opacity-60">Файлов нет</p>
                        )}
                        {cfFiles.files.map((f) => {
                          const sub = cfClass === 12 ? "resourcepacks" : cfClass === 6556 ? "shaderpacks" : "mods";
                          return (
                            <button
                              key={f.id}
                              className="w-full cursor-pointer rounded-lg px-2 py-1.5 text-left hover:bg-white/10"
                              disabled={dl !== null}
                              onClick={() => void cfInstallFile(p, f, sub)}
                            >
                              <div className="truncate text-[12px] font-semibold">{f.displayName || f.fileName}</div>
                              <div className="flex flex-wrap items-center gap-1.5 text-[10px] opacity-60">
                                <span>{new Date(f.fileDate).toLocaleDateString("ru")}</span>
                                <span className="font-mono-console">{f.gameVersions.slice(0, 3).join(", ")}</span>
                                {f.releaseType === 1 && <span className="badge !px-1.5 !py-0 text-[9px]">release</span>}
                              </div>
                            </button>
                          );
                        })}
                      </div>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )
      ) : loading || !hits ? (
        <CardGridSkeleton />
      ) : hits.length === 0 ? (
        <div className="glass grid place-items-center gap-2 p-14 text-center opacity-70">
          <Sparkles size={28} style={{ color: "var(--accent)" }} />
          <p className="text-[13px]">Ничего не найдено — попробуйте другой запрос или снимите фильтр категории.</p>
        </div>
      ) : (
        <>
          <div className="text-[11.5px] opacity-45">
            {hits.length} из {total} · API Modrinth v2
          </div>
          <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-3">
            {hits.map((h) => (
              <div key={h.project_id} className="glass card-hover flex gap-3.5 p-4">
                {h.icon_url ? (
                  <img src={h.icon_url} alt="" loading="lazy" className="size-14 shrink-0 rounded-xl object-cover" />
                ) : (
                  <div className="grid size-14 shrink-0 place-items-center rounded-xl" style={{ background: "var(--surface-strong)" }}>
                    <Sparkles size={20} style={{ color: "var(--accent)" }} />
                  </div>
                )}
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[14px] font-bold">{h.title}</div>
                  <div className="truncate text-[11px] opacity-55">by {h.author}</div>
                  <p className="mt-1 line-clamp-2 text-[11.5px] leading-snug opacity-70">{h.description}</p>
                  <div className="mt-2 flex items-center gap-2">
                    <span className="flex items-center gap-1 text-[11px] opacity-60">
                      <ArrowDownWideNarrow size={11} style={{ color: "var(--accent)" }} />
                      {Intl.NumberFormat("ru", { notation: "compact" }).format(h.downloads)}
                    </span>
                    <span className="flex-1" />
                    <button
                      className={`btn !py-1 !px-2.5 text-[12px] ${done.has(h.project_id) ? "" : "btn-primary"}`}
                      disabled={dl !== null}
                      onClick={() => void download(h)}
                    >
                      {dl === h.project_id ? (
                        <span className="spinner !size-3.5" />
                      ) : done.has(h.project_id) ? (
                        <Check size={13} />
                      ) : (
                        <Download size={13} />
                      )}
                      {done.has(h.project_id)
                        ? "готово"
                        : type === "modpack"
                          ? "установить"
                          : "скачать"}
                    </button>
                  </div>
                  {picker === h.project_id && packVers.length > 0 && (
                    <div className="mt-2 rounded-xl border border-white/10 bg-black/25 p-1.5">
                      <div className="mb-1 flex items-center gap-1.5 px-1 pt-0.5 text-[10.5px] opacity-60">
                        <Layers size={11} style={{ color: "var(--accent)" }} />
                        <span className="flex-1">выберите версию сборки ({packVers.length})</span>
                        <button
                          className="cursor-pointer hover:opacity-100"
                          aria-label="Закрыть список версий"
                          onClick={() => {
                            setPicker(null);
                            setPackVers([]);
                          }}
                        >
                          <X size={12} />
                        </button>
                      </div>
                      <div className="max-h-52 space-y-1 overflow-y-auto">
                        {[...packVers]
                          .sort(
                            (a, b) =>
                              Number(b.game_versions.includes(selected?.config.version ?? "")) -
                              Number(a.game_versions.includes(selected?.config.version ?? "")) ||
                              Date.parse(b.date_published) - Date.parse(a.date_published),
                          )
                          .map((v) => (
                            <button
                              key={v.id}
                              className="w-full cursor-pointer rounded-lg px-2 py-1.5 text-left hover:bg-white/10"
                              disabled={dl !== null}
                              onClick={() => void installPack(h, v)}
                            >
                              <div className="truncate text-[12px] font-semibold">{v.name}</div>
                              <div className="flex flex-wrap items-center gap-1.5 text-[10px] opacity-60">
                                <span className="badge !px-1.5 !py-0 text-[9.5px]">{v.loaders.join("/")}</span>
                                <span className="font-mono-console">
                                  MC {v.game_versions.slice(0, 3).join(", ")}
                                  {v.game_versions.length > 3 ? "…" : ""}
                                </span>
                              </div>
                            </button>
                          ))}
                      </div>
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
