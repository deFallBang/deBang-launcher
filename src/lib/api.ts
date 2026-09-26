import { invoke } from "@tauri-apps/api/core";

export interface MemInfo {
  totalMb: number;
  availableMb: number;
}

export interface JavaInstall {
  path: string;
  version: string;
  major: number;
  source: string;
}

export type ProxyKind = "None" | "Socks5" | "Http";

export interface ProxyConfig {
  kind: ProxyKind;
  host: string;
  port: number;
  login: string;
  password: string;
}

export interface InstanceConfig {
  id: string;
  name: string;
  version: string;
  loader: string;
  created: string;
  uuid: string;
  /** per-instance proxy (absent in configs written by older versions) */
  proxy?: ProxyConfig;
  autoGc?: boolean;
  autoMem?: boolean;
}

export interface ModEntry {
  file: string;
  name: string;
  size: number;
  enabled: boolean;
  broken: boolean;
}

export interface LaunchPlan {
  javaMajor: number;
  minMemMb: number;
  maxMemMb: number;
  autoGc: boolean;
  autoMem: boolean;
  autoGcApplied: boolean;
  gcFlags: string[];
  proxyEnabled: boolean;
  notes: string[];
}

export interface InstanceInfo {
  config: InstanceConfig;
  dir: string;
  modCount: number;
  hasRunScript: boolean;
}

export interface CfProject {
  id: number;
  name: string;
  slug: string;
  summary: string;
  downloadCount: number;
  logo?: { thumbnailUrl?: string; url?: string } | null;
  links?: { websiteUrl?: string; sourceUrl?: string } | null;
}

export interface CfFile {
  id: number;
  displayName?: string;
  fileName: string;
  downloadCount: number;
  gameVersions: string[];
  releaseType: number;
  fileDate: string;
}

export interface McVersion {
  id: string;
  type: string;
  releaseTime: string;
  url: string;
}

export interface MrVersion {
  id: string;
  name: string;
  version_number: string;
  loaders: string[];
  game_versions: string[];
  date_published: string;
  files: Array<{ url: string; filename: string; size: number }>;
}

export interface ModrinthHit {
  project_id: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  downloads: number;
  icon_url: string | null;
  categories: string[];
  project_type: string;
}

export interface LaunchSettings {
  playerName: string;
  javaPath: string;
  minMemMb: number;
  maxMemMb: number;
  jvmArgs: string[];
}

export const api = {
  memInfo: () => invoke<MemInfo>("get_mem_info"),
  detectJava: () => invoke<JavaInstall[]>("detect_java"),
  checkJava: (path: string) => invoke<string>("check_java_version", { path }),
  modrinthSearch: (query: string, index: string, facets: string, limit: number, offset: number) =>
    invoke<{ hits: ModrinthHit[]; total_hits?: number }>("modrinth_search", { query, index, facets, limit, offset }),
  modrinthVersions: (projectId: string, loaders: string, gameVersions: string) =>
    invoke<MrVersion[]>("modrinth_project_versions", { projectId, loaders, gameVersions }),
  mojangVersions: () => invoke<McVersion[]>("mojang_versions"),

  // ---- CurseForge ----
  cfKeyStatus: () =>
    invoke<{ configured: boolean; masked: string; source: string }>("curseforge_key_status"),
  cfKeySave: (key: string) =>
    invoke<{ configured: boolean; masked: string; source: string }>("curseforge_key_save", { key }),
  cfSearch: (args: {
    key: string;
    searchFilter: string;
    classId: number;
    gameVersion?: string | null;
    page?: number;
  }) => invoke<{ data: CfProject[] }>("curseforge_search", args),
  cfFiles: (key: string, projectId: number) =>
    invoke<{ data: CfFile[] }>("curseforge_files", { key, projectId }),
  cfDownloadFile: (args: {
    key: string;
    instanceId: string;
    fileId: number;
    filename: string;
    sub?: string | null;
  }) => invoke<string>("curseforge_download_file", args),
  cfInstallModpack: (key: string, fileId: number, name: string) =>
    invoke<string>("curseforge_install_modpack", { key, fileId, name }),
  sysInfo: () =>
    invoke<{
      session: string;
      os: string;
      kernel: string;
      arch: string;
      renderer: string;
      dataDir: string;
      launcherVersion: string;
    }>("get_sys_info"),
  listInstances: () => invoke<InstanceInfo[]>("list_instances"),
  createInstance: (name: string, version: string, loader: string) =>
    invoke<InstanceInfo>("create_instance", { name, version, loader }),
  deleteInstance: (id: string) => invoke<void>("delete_instance", { id }),
  downloadMod: (instanceId: string, url: string, filename: string, sub?: string) =>
    invoke<string>("download_mod", { instanceId, url, filename, sub: sub ?? null }),
  importRunFile: (instanceId: string, src: string) =>
    invoke<string>("import_run_file", { instanceId, src }),
  importBackground: (src: string) => invoke<string>("import_background", { src }),
  updateInstanceSettings: (
    instanceId: string,
    p: { proxy?: ProxyConfig; autoGc?: boolean; autoMem?: boolean },
  ) => invoke<InstanceInfo>("update_instance_settings", { instanceId, ...p }),
  instanceLaunchPlan: (instanceId: string, settings: LaunchSettings) =>
    invoke<LaunchPlan>("instance_launch_plan", { instanceId, settings }),
  listInstanceMods: (instanceId: string) => invoke<ModEntry[]>("list_instance_mods", { instanceId }),
  toggleInstanceMod: (instanceId: string, file: string, enable: boolean) =>
    invoke<void>("toggle_instance_mod", { instanceId, file, enable }),
  deleteInstanceMod: (instanceId: string, file: string) =>
    invoke<void>("delete_instance_mod", { instanceId, file }),
  openInstanceFolder: (instanceId: string) => invoke<void>("open_instance_folder", { instanceId }),
  installModpack: (url: string, filename: string, name: string) =>
    invoke<string>("install_modpack", { url, filename, name }),
  launch: (instanceId: string, settings: LaunchSettings) =>
    invoke<number>("launch_instance", { instanceId, settings }),
  stop: (instanceId: string) => invoke<void>("stop_instance", { instanceId }),
  cancelDownload: () => invoke<void>("cancel_download"),
};
