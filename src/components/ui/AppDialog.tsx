// ============================================================
// Zentrale, wiederverwendbare Dialogverwaltung (TICKET-012).
//
// Ersetzt die frühere Verwaltung separater Tauri-Fenster: Dialoge
// werden als React-Komponenten innerhalb des Hauptfensters dargestellt
// (Radix Dialog), immer zentriert geöffnet, sind per Ziehgriff unten
// rechts vergrößer-/verkleinerbar (Grenzen: pro Dialog definiertes
// Minimum bis 90 % der Hauptfenstergröße) und merken sich ihre zuletzt
// verwendete Größe pro Dialogtyp (`dialogKey`) über Neustarts hinweg
// (nicht die Position — die wird immer neu zentriert).
//
// Modalität, Fokus-Falle/-Wiederherstellung und `Esc` zum Schließen
// kommen von Radix' `Dialog.Root` (Standard `modal={true}`) — nicht
// modale Dialoge müssten das ausdrücklich mit `modal={false}` abwählen,
// was aktuell kein Dialog dieser App benötigt.
// ============================================================

import type { MouseEvent as ReactMouseEvent, ReactNode } from "react";
import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useDialogSizeStore, type DialogSize } from "@/store/dialogSizeStore";

interface Props {
  /** Stabile Kennung des Dialogtyps — Schlüssel für die Größenpersistenz. */
  dialogKey: string;
  open: boolean;
  onClose: () => void;
  /** Inhalt der Titelzeile (Icon + Text + evtl. Badges). */
  titleBar: ReactNode;
  /** Optionale Fußzeile (Buttons, Zähler …). */
  footer?: ReactNode;
  defaultSize: DialogSize;
  /** Minimale Größe; Standard = `defaultSize`. */
  minSize?: DialogSize;
  /** Kein Ziehgriff — feste Größe (z. B. wenn sich der Inhalt nicht
   *  sinnvoll skalieren lässt). Standard: vergrößerbar. */
  fixedSize?: boolean;
  children: ReactNode;
}

const MAX_FRAC = 0.9; // maximal 90 % der Hauptfenstergröße

function clamp(size: DialogSize, min: DialogSize): DialogSize {
  const maxW = Math.max(min.w, window.innerWidth * MAX_FRAC);
  const maxH = Math.max(min.h, window.innerHeight * MAX_FRAC);
  return {
    w: Math.min(maxW, Math.max(min.w, size.w)),
    h: Math.min(maxH, Math.max(min.h, size.h)),
  };
}

function ResizeHandle({ onResize }: { onResize: (e: ReactMouseEvent) => void }) {
  return (
    <div
      onMouseDown={onResize}
      className="absolute bottom-0 right-0 z-10 flex h-4 w-4 cursor-nwse-resize items-end justify-end"
    >
      <svg width="10" height="10" viewBox="0 0 10 10" className="text-dim">
        <g stroke="currentColor" strokeWidth="1" strokeLinecap="round" opacity="0.6">
          <line x1="9" y1="2" x2="2" y2="9" />
          <line x1="9" y1="5" x2="5" y2="9" />
          <line x1="9" y1="8" x2="8" y2="9" />
        </g>
      </svg>
    </div>
  );
}

export function AppDialog({
  dialogKey,
  open,
  onClose,
  titleBar,
  footer,
  defaultSize,
  minSize,
  fixedSize,
  children,
}: Props) {
  const min = minSize ?? defaultSize;
  // Bewusst kein reaktiver Store-Hook (`useDialogSizeStore(selector)`) hier:
  // bei mehreren gleichzeitig gemounteten `AppDialog`-Instanzen (alle Dialoge
  // hängen unbedingt in App.tsx, nur `open` unterscheidet sie) löst die
  // Persist-Rehydrierung sonst ein „setState während des Renderns einer
  // anderen Komponente"-Warning aus — auch bei einem stabilen Actions-
  // Selektor, da schon das Benachrichtigen aller Subscriber beim
  // Rehydrieren zählt. `size` wird lokal per `useState` gehalten; der
  // Store dient nur als reines Lese-/Schreib-Depot über `.getState()`.
  const [size, setSize] = useState<DialogSize>(() =>
    clamp(useDialogSizeStore.getState().sizes[dialogKey] ?? defaultSize, min),
  );

  // Beim Öffnen: gespeicherte Größe (auf Hauptfenster/Minimum geklemmt)
  // übernehmen — ein zu klein gewordenes Hauptfenster setzt den Dialog
  // vollständig in den sichtbaren Bereich zurück.
  useEffect(() => {
    if (!open) return;
    const stored = useDialogSizeStore.getState().sizes[dialogKey];
    setSize(clamp(stored ?? defaultSize, min));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Hauptfenster wird währenddessen verkleinert: Dialog im sichtbaren
  // Bereich halten.
  useEffect(() => {
    if (!open) return;
    const onWindowResize = () => setSize((s) => clamp(s, min));
    window.addEventListener("resize", onWindowResize);
    return () => window.removeEventListener("resize", onWindowResize);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Ziehgriff unten rechts. Da der Dialog zentriert ist (translate -50 %),
  // wächst er um das Doppelte der Mausbewegung, damit die Ecke dem Cursor
  // folgt. Die Größe wird erst beim Loslassen persistiert.
  const startResize = (e: ReactMouseEvent) => {
    e.preventDefault();
    const sx = e.clientX;
    const sy = e.clientY;
    const sw = size.w;
    const sh = size.h;
    const onMove = (ev: MouseEvent) => {
      setSize(
        clamp({ w: sw + (ev.clientX - sx) * 2, h: sh + (ev.clientY - sy) * 2 }, min),
      );
    };
    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      setSize((current) => {
        useDialogSizeStore.getState().setSize(dialogKey, current);
        return current;
      });
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
  };

  return (
    <Dialog.Root open={open} onOpenChange={(o) => !o && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/50" />
        <Dialog.Content
          style={{ width: size.w, height: size.h }}
          className="fixed left-1/2 top-1/2 z-50 flex max-h-[90vh] max-w-[90vw] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-lg border border-edge bg-panel shadow-2xl"
          aria-describedby={undefined}
        >
          <Dialog.Title asChild>
            <div className="flex flex-shrink-0 items-center gap-2 border-b border-edge bg-header px-4 py-2 text-text">
              {titleBar}
            </div>
          </Dialog.Title>
          {children}
          {footer}
          {!fixedSize && <ResizeHandle onResize={startResize} />}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
