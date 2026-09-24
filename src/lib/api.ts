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

export interface InstanceConfig {
  id: string;
  name: string;
  version: string;
  loader: string;
  created: string;
  uuid: string;
}

export interface InstanceInfo {
  config: InstanceConfig;
  dir: string;
  modCount: number;
  hasRunScript: boolean;
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
  installModpack: (url: string, filename: string, name: string) =>
    invoke<string>("install_modpack", { url, filename, name }),
  launch: (instanceId: string, settings: LaunchSettings) =>
    invoke<number>("launch_instance", { instanceId, settings }),
  stop: (instanceId: string) => invoke<void>("stop_instance", { instanceId }),
  cancelDownload: () => invoke<void>("cancel_download"),
};
