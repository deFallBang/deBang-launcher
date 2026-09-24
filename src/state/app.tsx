import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import themeData from "../theme/theme.json";
import { api, type InstanceInfo, type MemInfo } from "../lib/api";

export interface LogLine {
  time: number;
  line: string;
  stream: string;
}

export interface GameStatus {
  running: boolean;
  instanceId: string;
  code?: number | null;
}

export interface PrepState {
  phase: string;
  done: number;
  total: number;
}

export type BgType = "gradient" | "image" | "video";

export interface SysInfo {
  session: string;
  os: string;
  kernel: string;
  arch: string;
  renderer: string;
  dataDir: string;
  launcherVersion: string;
}

export interface Settings {
  preset: string;
  playerName: string;
  accent: string | null;
  radius: number;
  bgType: BgType;
  bgPath: string;
  blur: number;
  dim: number;
  javaPath: string;
  minMem: number;
  maxMem: number;
  jvmPreset: "aikar" | "zgc" | "shenandoah" | "vanilla";
  customArgs: string;
  selectedInstance: string | null;
}

export const JVM_PRESETS: Record<Settings["jvmPreset"], { name: string; args: string }> = {
  aikar: {
    name: "Aikar's Flags",
    args:
      "-XX:+UseG1GC -XX:+ParallelRefProcEnabled -XX:MaxGCPauseMillis=200 -XX:+UnlockExperimentalVMOptions -XX:+DisableExplicitGC -XX:G1NewSizePercent=40 -XX:G1MaxNewSizePercent=50 -XX:G1HeapRegionSize=16M -XX:G1ReservePercent=15 -XX:G1HeapWastePercent=5 -XX:G1MixedGCCountTarget=4 -XX:InitiatingHeapOccupancyPercent=20 -XX:G1MixedGCLiveThresholdPercent=90 -XX:G1RSetUpdatingPauseTimePercent=5 -XX:SurvivorRatio=32 -XX:+PerfDisableSharedMem -XX:MaxTenuringThreshold=1",
  },
  zgc: {
    name: "ZGC (low latency)",
    args: "-XX:+UseZGC -XX:+ZGenerational -XX:+AlwaysPreTouch -XX:+PerfDisableSharedMem -XX:-UseAdaptiveSizePolicy",
  },
  shenandoah: {
    name: "Shenandoah",
    args: "-XX:+UseShenandoahGC -XX:ShenandoahGCMode=satb -XX:+AlwaysPreTouch -XX:-UseBiasedLocking",
  },
  vanilla: { name: "Vanilla", args: "" },
};

const DEFAULTS: Settings = {
  preset: "catppuccin",
  playerName: "deBangPlayer",
  accent: null,
  radius: 16,
  bgType: "gradient",
  bgPath: "",
  blur: 16,
  dim: 45,
  javaPath: "",
  minMem: 2048,
  maxMem: 4096,
  jvmPreset: "aikar",
  customArgs: "",
  selectedInstance: null,
};

interface AppCtx {
  settings: Settings;
  patch: (p: Partial<Settings>) => void;
  presets: typeof themeData.presets;
  mem: MemInfo | null;
  memFailed: boolean;
  instancesFailed: boolean;
  sys: SysInfo | null;
  instances: InstanceInfo[];
  refreshInstances: () => Promise<void>;
  logs: LogLine[];
  clearLogs: () => void;
  status: GameStatus | null;
  prep: PrepState | null;
  toast: (msg: string, kind?: "ok" | "err") => void;
  toasts: Array<{ id: number; msg: string; kind: string }>;
  assembledArgs: string[];
}

const Ctx = createContext<AppCtx>(null as never);
export const useApp = () => useContext(Ctx);

const NAME_RE = /^[A-Za-z0-9_]{3,16}$/;
export const isValidPlayerName = (n: string) => NAME_RE.test(n);

function clampInt(v: unknown, min: number, max: number, fallback: number): number {
  const n = typeof v === "number" && Number.isFinite(v) ? Math.round(v) : fallback;
  return Math.min(max, Math.max(min, n));
}

function str(v: unknown, fallback: string): string {
  return typeof v === "string" ? v : fallback;
}

/// Never trusts localStorage blindly: a corrupted or legacy blob used to crash
/// the UI (invalid jvmPreset) or start Minecraft with nonsense memory values.
function load(): Settings {
  try {
    let raw: Record<string, unknown> = {};
    const cur = localStorage.getItem("debang.settings");
    const legacy = localStorage.getItem("cachymc.settings");
    const parsed = JSON.parse(cur ?? legacy ?? "{}");
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
      raw = parsed as Record<string, unknown>;
    }
    const out: Settings = {
      preset:
        typeof raw.preset === "string" && themeData.presets.some((p) => p.id === raw.preset)
          ? raw.preset
          : DEFAULTS.preset,
      playerName: isValidPlayerName(str(raw.playerName, "")) ? (raw.playerName as string) : DEFAULTS.playerName,
      accent: typeof raw.accent === "string" ? raw.accent : DEFAULTS.accent,
      radius: clampInt(raw.radius, 0, 32, DEFAULTS.radius),
      bgType:
        raw.bgType === "image" || raw.bgType === "video" || raw.bgType === "gradient"
          ? raw.bgType
          : DEFAULTS.bgType,
      bgPath: str(raw.bgPath, DEFAULTS.bgPath),
      blur: clampInt(raw.blur, 0, 40, DEFAULTS.blur),
      dim: clampInt(raw.dim, 0, 90, DEFAULTS.dim),
      javaPath: str(raw.javaPath, DEFAULTS.javaPath),
      minMem: clampInt(raw.minMem, 512, 65536, DEFAULTS.minMem),
      maxMem: clampInt(raw.maxMem, 512, 65536, DEFAULTS.maxMem),
      jvmPreset:
        raw.jvmPreset === "aikar" ||
        raw.jvmPreset === "zgc" ||
        raw.jvmPreset === "shenandoah" ||
        raw.jvmPreset === "vanilla"
          ? raw.jvmPreset
          : DEFAULTS.jvmPreset,
      customArgs: str(raw.customArgs, DEFAULTS.customArgs),
      selectedInstance:
        typeof raw.selectedInstance === "string" && raw.selectedInstance
          ? raw.selectedInstance
          : DEFAULTS.selectedInstance,
    };
    if (out.maxMem < out.minMem) out.maxMem = out.minMem;
    return out;
  } catch {
    return { ...DEFAULTS };
  }
}

export function AppProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<Settings>(load);
  const [mem, setMem] = useState<MemInfo | null>(null);
  const [instances, setInstances] = useState<InstanceInfo[]>([]);
  const [memFailed, setMemFailed] = useState(false);
  const [sys, setSys] = useState<SysInfo | null>(null);
  const [instancesFailed, setInstancesFailed] = useState(false);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [status, setStatus] = useState<GameStatus | null>(null);
  const [prep, setPrep] = useState<PrepState | null>(null);
  const [toasts, setToasts] = useState<Array<{ id: number; msg: string; kind: string }>>([]);
  const cap = useRef(600);

  const patch = useCallback((p: Partial<Settings>) => {
    setSettings((s) => {
      const next = { ...s, ...p };
      localStorage.setItem("debang.settings", JSON.stringify(next));
      return next;
    });
  }, []);

  const toast = useCallback((msg: string, kind: "ok" | "err" = "ok") => {
    const id = Date.now() + Math.random();
    setToasts((t) => [...t.slice(-2), { id, msg, kind }]);
    setTimeout(() => setToasts((t) => t.filter((x) => x.id !== id)), 3200);
  }, []);

  useEffect(() => {
    localStorage.setItem("debang.settings", JSON.stringify(settings));
  }, [settings]);

  // theme variables
  useEffect(() => {
    const preset = themeData.presets.find((p) => p.id === settings.preset) ?? themeData.presets[0];
    const root = document.documentElement.style;
    Object.entries(preset.vars).forEach(([k, v]) => root.setProperty(k, v as string));
    if (settings.accent) {
      root.setProperty("--accent", settings.accent);
      const light = luminance(settings.accent) > 0.55;
      root.setProperty("--accent-fg", light ? "#11111b" : "#ffffff");
    }
    root.setProperty("--radius", `${settings.radius}px`);
    root.setProperty("--bg-blur", `${settings.blur}px`);
    root.setProperty("--bg-dim", `${settings.dim / 100}`);
  }, [settings.preset, settings.accent, settings.radius, settings.blur, settings.dim]);

  // system polling
  useEffect(() => {
    const tick = () => {
      api
        .memInfo()
        .then((m) => {
          setMem(m);
          setMemFailed(false);
        })
        .catch(() => setMemFailed(true));
    };
    tick();
    const t = setInterval(tick, 4000);
    return () => clearInterval(t);
  }, []);

  const refreshInstances = useCallback(async () => {
    try {
      const list = await api.listInstances();
      setInstances(list);
      setInstancesFailed(false);
      setSettings((s) => {
        if (s.selectedInstance && !list.some((i) => i.config.id === s.selectedInstance)) {
          const next = { ...s, selectedInstance: list[0]?.config.id ?? null };
          localStorage.setItem("debang.settings", JSON.stringify(next));
          return next;
        }
        if (!s.selectedInstance && list.length) {
          const next = { ...s, selectedInstance: list[0].config.id };
          localStorage.setItem("debang.settings", JSON.stringify(next));
          return next;
        }
        return s;
      });
    } catch (e) {
      console.error(e);
      setInstancesFailed(true);
    }
  }, []);

  useEffect(() => {
    void refreshInstances();
    api
      .sysInfo()
      .then(setSys)
      .catch((e) => console.error("get_sys_info", e));
  }, [refreshInstances]);

  // game events
  useEffect(() => {
    const un1 = listen<LogLine>("game://log", (e) =>
      setLogs((l) => {
        const next = [...l, e.payload];
        return next.length > cap.current ? next.slice(next.length - cap.current) : next;
      }),
    ).catch((e) => {
      console.error("game://log", e);
      return () => {};
    });
    const un2 = listen<GameStatus>("game://status", (e) => {
      setStatus(e.payload);
      if (e.payload.running) setPrep(null);
    }).catch((e) => {
      console.error("game://status", e);
      return () => {};
    });
    const un3 = listen<PrepState>("prep://progress", (e) => {
      setPrep(e.payload.phase === "done" ? null : e.payload);
    }).catch((e) => {
      console.error("prep://progress", e);
      return () => {};
    });
    void invoke<boolean>("is_game_running")
      .then((r) => {
        if (r) setStatus({ running: true, instanceId: "", code: null });
      })
      .catch((e) => console.error("is_game_running", e));
    return () => {
      void un1.then((f) => f()).catch(() => {});
      void un2.then((f) => f()).catch(() => {});
      void un3.then((f) => f()).catch(() => {});
    };
  }, []);

  const clearLogs = useCallback(() => setLogs([]), []);

  const assembledArgs = useMemo(() => {
    const preset = JVM_PRESETS[settings.jvmPreset].args;
    return [...(preset ? preset.split(/\s+/) : []), ...settings.customArgs.split(/\s+/).filter(Boolean)];
  }, [settings.jvmPreset, settings.customArgs]);

  const value = useMemo<AppCtx>(
    () => ({
      settings,
      patch,
      presets: themeData.presets,
      mem,
      memFailed,
      instancesFailed,
      sys,
      instances,
      refreshInstances,
      logs,
      clearLogs,
      status,
      prep,
      toast,
      toasts,
      assembledArgs,
    }),
    [
      settings,
      patch,
      clearLogs,
      mem,
      memFailed,
      instancesFailed,
      sys,
      instances,
      refreshInstances,
      logs,
      status,
      prep,
      toast,
      toasts,
      assembledArgs,
    ],
  );

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

function luminance(hex: string): number {
  const m = hex.replace("#", "");
  const full = m.length === 3 ? m.split("").map((c) => c + c).join("") : m;
  const n = parseInt(full.slice(0, 6), 16);
  const [r, g, b] = [(n >> 16) & 255, (n >> 8) & 255, n & 255].map(
    (v) => v / 255 <= 0.03928 ? v / 255 / 12.92 : Math.pow((v / 255 + 0.055) / 1.055, 2.4),
  );
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}
