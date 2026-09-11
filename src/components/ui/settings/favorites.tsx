// ============================================================
// Settings ▸ Favorites (TICKET-035).
//
// The favourite folders the menu and the toolbar offer.
// ============================================================

import { panelOf, usePanes } from "@/store/panesStore";
import { useSettings, useT } from "@/store/settingsStore";
import { useState } from "react";
import {
  Plus,
  Star,
  Trash2,
} from "lucide-react";

function basename(path: string): string {
  if (!path || path === "/") return path || "/";
  const parts = path.replace(/[\\/]+$/, "").split(/[\\/]/);
  return parts[parts.length - 1] || path;
}

export function FavoritesForm() {
  const t = useT();
  const favorites = useSettings((s) => s.favorites);
  const addFavorite = useSettings((s) => s.addFavorite);
  const removeFavorite = useSettings((s) => s.removeFavorite);
  const currentPath = usePanes((s) => panelOf(s, s.active).path);

  const [path, setPath] = useState("");
  const [name, setName] = useState("");

  const effectivePath = (path || currentPath).trim();
  const effectiveName = (name || basename(effectivePath)).trim();

  const add = () => {
    if (!effectivePath) return;
    addFavorite({ name: effectiveName || effectivePath, path: effectivePath });
    setPath("");
    setName("");
  };

  return (
    <div className="flex flex-col gap-4">
      <p className="text-[11px] text-dim">{t("settings.favorites.hint")}</p>

      {/* Existing favorites */}
      {favorites.length === 0 ? (
        <p className="text-[12px] text-dim">{t("settings.favorites.empty")}</p>
      ) : (
        <ul className="flex flex-col gap-1">
          {favorites.map((f) => (
            <li
              key={f.path}
              className="flex items-center gap-2 rounded border border-edge bg-panel-inactive px-2 py-1"
            >
              <Star size={14} className="shrink-0 text-amber-400" />
              <div className="flex min-w-0 flex-col leading-tight">
                <span className="truncate text-[12px] text-text">{f.name}</span>
                <span className="truncate font-mono text-[10px] text-dim">
                  {f.path}
                </span>
              </div>
              <button
                onClick={() => removeFavorite(f.path)}
                className="ml-auto rounded p-1 text-dim hover:bg-red-500/20 hover:text-red-400"
                title={t("op.cancel")}
              >
                <Trash2 size={13} />
              </button>
            </li>
          ))}
        </ul>
      )}

      {/* Add a new favorite */}
      <div className="flex flex-col gap-2 border-t border-edge pt-3">
        <input
          value={path}
          onChange={(e) => setPath(e.target.value)}
          placeholder={`${t("settings.favorites.path")} (${currentPath})`}
          spellCheck={false}
          className="rounded border border-edge bg-panel-inactive px-2 py-1 font-mono text-[12px] text-text outline-none focus:border-accent"
        />
        <div className="flex gap-2">
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder={`${t("settings.favorites.name")} (${basename(effectivePath)})`}
            spellCheck={false}
            className="min-w-0 flex-1 rounded border border-edge bg-panel-inactive px-2 py-1 text-[12px] text-text outline-none focus:border-accent"
          />
          <button
            onClick={add}
            className="flex items-center gap-1.5 rounded bg-accent px-3 py-1 text-[12px] text-white hover:brightness-110"
          >
            <Plus size={13} />
            {t("settings.favorites.add")}
          </button>
        </div>
      </div>
    </div>
  );
}
