import type { FileIconRequest } from "../bindings/FileIconRequest";
import type { FileIconResult } from "../bindings/FileIconResult";
import { hasTauri, invoke } from "../invoke";

export function fileIcons(requests: FileIconRequest[]): Promise<FileIconResult[]> {
  return hasTauri() ? invoke("file_icons", { requests }) : Promise.resolve([]);
}
