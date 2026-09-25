// ============================================================
// Properties, permissions and sizes (TICKET-035).
//
// What the properties dialog needs: the entry's own metadata, its
// rights and owner, its checksums, its Finder tags, the recursive size
// of a folder and the volume it lies on.
// ============================================================

import type {
  Checksums,
  DirSize,
  DirSizeStats,
  FileProps,
  Tag,
  VolumeInfo,
} from "../bindings";
import { hasTauri, invoke } from "../invoke";
import * as demo from "../demo";

/**
 * Sums a folder recursively in the background and streams intermediate
 * totals over `onProgress`. `token` identifies the task for `cancelDirSize`.
 */
export async function dirSize(
  path: string,
  token: string,
  onProgress: (stats: DirSizeStats) => void,
): Promise<DirSize> {
  if (!hasTauri()) return demo.demoDirSize(onProgress);
  const { Channel } = await import("@tauri-apps/api/core");
  const channel = new Channel<DirSizeStats>();
  channel.onmessage = onProgress;
  return invoke<DirSize>("dir_size", { path, token, onProgress: channel });
}

/** Stops a running `dirSize`; unknown tokens are ignored. */
export async function cancelDirSize(token: string): Promise<void> {
  if (!hasTauri()) return;
  return invoke<void>("cancel_dir_size", { token });
}

/** Capacity/usage of the volume `path` lies on; null if none matches. */
export async function volumeInfo(path: string): Promise<VolumeInfo | null> {
  if (!hasTauri()) return demo.demoVolume();
  return invoke<VolumeInfo | null>("volume_info", { path });
}

/** Reads permissions, owner, extended attributes, and ACL of an entry. */
export async function fileProps(path: string): Promise<FileProps> {
  if (!hasTauri()) return demo.demoProps(path);
  return invoke<FileProps>("file_props", { path });
}

/** Sets the access rights (chmod); `mode` are the permission bits (0..0o7777). */
export async function setPermissions(path: string, mode: number): Promise<void> {
  if (!hasTauri()) {
    console.log("[demo] set_permissions", path, mode.toString(8));
    return;
  }
  return invoke<void>("set_permissions", { path, mode });
}

/** Changes owner/group (chown); name or numeric ID, null = unchanged. */
export async function setOwner(
  path: string,
  owner: string | null,
  group: string | null,
): Promise<void> {
  if (!hasTauri()) {
    console.log("[demo] set_owner", path, owner, group);
    return;
  }
  return invoke<void>("set_owner", { path, owner, group });
}

/** Computes MD5, SHA-1, and SHA-256 of a file. */
export async function fileChecksums(path: string): Promise<Checksums> {
  if (!hasTauri()) {
    return {
      md5: "d41d8cd98f00b204e9800998ecf8427e",
      sha1: "da39a3ee5e6b4b0d3255bfef95601890afd80709",
      sha256:
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    };
  }
  return invoke<Checksums>("file_checksums", { path });
}

/** Reads the Finder tags of an entry (macOS; otherwise empty). */
export async function getTags(path: string): Promise<Tag[]> {
  if (!hasTauri()) return demo.demoGetTags();
  return invoke<Tag[]>("get_tags", { path });
}

/** Writes the Finder tags (an empty list removes them). */
export async function setTags(path: string, tags: Tag[]): Promise<void> {
  if (!hasTauri()) {
    demo.demoSetTags(tags);
    console.log("[demo] set_tags", path, tags);
    return;
  }
  return invoke<void>("set_tags", { path, tags });
}
