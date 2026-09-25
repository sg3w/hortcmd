import type { LocationView } from "../bindings/LocationView";
import { hasTauri, invoke } from "../invoke";
import { locationIdentity, locationUri, migrateLegacyLocation, parseLocation, storeIdentity } from "@/lib/location";
import { joinPath, parentPath } from "@/lib/path";

/** Typed import/navigation boundary, with the same presentation codec in demo mode. */
export async function normalizeLocation(raw: string, legacy = false, child: string | null = null): Promise<LocationView> {
  if (hasTauri()) return invoke<LocationView>("vfs_location", { raw, legacy, child });
  let location = parseLocation(legacy ? migrateLegacyLocation(raw) : raw);
  if (child !== null) {
    if (location.location.kind !== "remote" || !child || child.includes("/") || child === "." || child === "..") throw new Error("Invalid remote child");
    location = parseLocation(joinPath(locationUri(location), child));
  }
  const uri = locationUri(location);
  return { location, uri, identity: locationIdentity(location), store: storeIdentity(location), parent: parentPath(uri) };
}
