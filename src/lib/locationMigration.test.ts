import { describe, expect, it } from "vitest";
import { migrateSettings } from "@/store/persisted/settings";
import { useSession } from "@/store/sessionStore";
import { useHistory } from "@/store/historyStore";
import { useDemo } from "@/test/tauri";

// Exercise the actual Zustand migration functions, not a copy of their algorithm.
describe("stored VFS locations", () => {
  it("migrates only old session/history/favorite paths once", async () => {
    useDemo();
    const path = "sftp://u@h/a%20b?c";
    const expected = "sftp://u@h/a%2520b%3Fc";
    const session = { session: { left: { tabs: [{ path }] } } };
    const sessionMigrate = useSession.persist.getOptions().migrate!;
    const migrated = await sessionMigrate(session, 1);
    expect(migrated).toEqual({ session: { left: { tabs: [{ path: expected }] } } });
    expect(await sessionMigrate(migrated, 2)).toEqual(migrated);
    const historyMigrate = useHistory.persist.getOptions().migrate!;
    expect(await historyMigrate({ recent: [path, "/tmp/%20"] }, 1)).toEqual({ recent: [expected, "/tmp/%20"] });
    const favorites = { favorites: [{ name: "kept", path }] };
    expect(migrateSettings(favorites, 1).favorites).toEqual([{ name: "kept", path: expected }]);
    const newSettings = migrateSettings(favorites, 1);
    expect(migrateSettings(newSettings, 2)).toEqual(newSettings);
  });
});
