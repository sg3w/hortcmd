# TICKET-062: Dateisuche zeigt beim Wiederöffnen abwechselnd leere und letzte Suche

## Fehlerbild

Nutzerbericht: Nach einer Dateisuche wird das Suchfenster geschlossen. Bei
wiederholtem Öffnen erscheint abwechselnd eine leere Ansicht und die letzte Suche:
**leer → letzte Suche → leer → letzte Suche**. Das Wiederöffnen darf keinen solchen
Toggle-Effekt erzeugen.

Noch zu verifizieren: Ob „leer“ nur die Ergebnisliste oder auch Suchfelder betrifft,
welcher Einstieg/Schließweg benutzt wurde und ob die Suche schon abgeschlossen war.
Diese Varianten bei der Reproduktion prüfen; keine Ursache als bewiesen voraussetzen.

## Reproduktion

1. Dateisuche in einem Verzeichnis mit bekannten Treffern öffnen.
2. Suchbegriff eingeben, Suche starten und auf Ergebnisse warten.
3. Dialog schließen und über denselben Einstieg erneut öffnen.
4. Mindestens sechs Öffnen-/Schließen-Zyklen ohne neue Suche durchführen.
5. Suchfelder, Ergebnisanzahl, tatsächlich gerenderte Zeilen, Scrollposition und
   Status pro Zyklus vergleichen; zusätzlich bei noch laufender Suche wiederholen.

## Erwartetes Verhalten

- Im selben Panel/Verzeichnis und Suchmodus bleibt die letzte Suche während der
  App-Sitzung konsistent sichtbar: Suchparameter, Ergebnisse und Abschlussstatus.
  Schließen und Öffnen allein löscht oder startet keine Suche.
- Bei einer abgebrochenen Suche dürfen bisherige Treffer erhalten bleiben, müssen
  aber als unvollständig/abgebrochen erkennbar sein. Kein dauerhaft laufender Spinner.
- Erster Aufruf ohne Suche zeigt den normalen leeren Anfangszustand.
- Wechsel von Verzeichnis/Panel oder Suchmodus darf alte Ergebnisse nicht unter
  neuen Suchparametern darstellen. Den Kontext der letzten Suche mitführen; bei
  verändertem Suchort/Modus Ergebnisse konsistent zurücksetzen und eine neue Suche
  erst auf expliziten Start ausführen. Suchtext/Optionen dürfen als Eingabe erhalten bleiben.
- Keine neue persistente Suchhistorie über App-Neustarts erforderlich.

## Ansatzpunkte aus der Codeprüfung

- `src/components/ui/SearchDialog.tsx` hält Suchparameter, Treffer und Status lokal.
  Initialisierung von Suchort/Modus erfolgt derzeit mit State-Updates in `useMemo`;
  Lebenszyklus und Abhängigkeiten auf Wiederöffnungsprobleme untersuchen.
- Die Trefferliste verwendet `useVirtualizer` mit `scrollRef`. Dialoginhalt und
  Scrollcontainer können beim Schließen unmounten, während Treffer erhalten bleiben.
  Prüfen, ob Anzahl/State korrekt sind, aber virtuelle Zeilen nach Wiederöffnung fehlen.
- `stopRunning()` fordert Backend-Abbruch an; `runToken` schützt eingehende Batches.
  Schließen/erneutes Öffnen und verspätete Batches/Abschlussmeldungen auf Rennen prüfen.
- `src/store/searchStore.ts` verwaltet `side` und `mode`;
  `src/App.tsx` bindet den Suchdialog ein. Mehrfache Öffnungsereignisse sowie
  Menü-/Shortcut-Routing und Fokuswiederherstellung ebenfalls prüfen.

Dies sind Prüfhypothesen, keine bestätigte Diagnose.

## Anforderungen

- Ursache reproduzieren und dokumentieren; kleinsten gezielten Fix im vorhandenen
  Dialog-/Store-/Virtualisierungsablauf umsetzen. Kein Umweg über wechselnde Keys,
  künstliche Wartezeiten oder unbedingtes Löschen der Treffer bei jeder Öffnung.
- Suchkontext, angezeigte Parameter und Resultate eindeutig zusammenhalten.
  Initialisierung bei Öffnung deterministisch gestalten, ohne Render-Seiteneffekte.
- Virtuelle Liste beim erneuten Mounten zuverlässig an den neuen Scrollcontainer
  binden und messen, falls dies die Ursache ist. Auch nach Scrollen in großen
  Ergebnislisten und Größenänderung müssen Zeilen wieder sichtbar sein.
- Schließen beendet eine laufende Suche wie bisher. Verspätete Antworten dürfen
  einen neu geöffneten Dialog oder einen neu gestarteten Suchlauf nicht überschreiben.
- Menü, Toolbar und Alt+F7 sowie Schließen über X/Escape, soweit unterstützt,
  und Schließen durch Öffnen eines Treffers konsistent behandeln.
- Tastaturbelegung und allgemeine Menü-/Fensterarchitektur unverändert lassen.

## Akzeptanzkriterien und Tests

- [x] Mindestens sechs Wiederöffnungen derselben abgeschlossenen Suche zeigen
      dieselben Suchparameter, Treffer und Status, ohne alternierende Leeransicht.
- [x] Regressionsfall mit virtualisierter großer Liste, vorherigem Scrollen und
      Dialog-Resize geprüft; vorhandene Treffer sind tatsächlich gerendert.
- [x] Keine Treffer, erster Aufruf, Fehler sowie abgebrochene Suche bleiben eindeutig.
- [x] Schließen während des Streams, sofortiges Wiederöffnen und Start einer neuen
      Suche sind mit verspäteten Batches/Abschlussmeldungen getestet.
- [x] Wechsel von Panel, Suchort und Suchmodus vermischt keine alten Ergebnisse
      mit neuem Kontext; keine automatische erneute Suche.
- [x] Alle vorhandenen Öffnungs-/Schließwege geprüft, einschließlich Fokus und
      fehlender doppelter Shortcut-Ausführung.
- [x] Automatischer Regressionstest bildet den reproduzierten Fehler ab;
      bei Layout-/Virtualisierungsursache zusätzlich Prüfung im echten Browser/Tauri.

## Definition of Done

- Ursache und Fix dokumentiert; relevante Tests, `cargo check` in `src-tauri`,
  `npm run typecheck` und `npm run build` bestehen.
- Erst nach Implementierung und Verifikation nach
  `tickets/done/062-search-dialog-reopen-toggle.done.md` verschieben.
- Lokaler Commit nach abgeschlossenem Ticket, ohne Push.

## Ursache und Abschluss

Reproduziert mit dem bisherigen Code: Nach vollständigem Unmount fehlen sowohl
Suchtext als auch Treffer. `LazyFeature` hält den Dialog 400 ms nach dem Schließen
noch gemountet. Rasches Wiederöffnen behält lokale States, späteres Wiederöffnen
verliert sie. Ein strikt alternierender Rhythmus unabhängig vom Zeitabstand wurde
nicht nachgewiesen. Der Regressionstest scheitert vor dem Fix bereits beim ersten
Wiederöffnen (Suchtext leer) und besteht danach über sechs Unmount-Zyklen.

Fix: Sitzungsspeicher außerhalb des Dialogs, Kontextinitialisierung beim Öffnen
statt State-Updates in `useMemo`. Keine persistente Historie und keine Änderungen
an LazyFeature, Tastatur-Routing oder Virtualisierung. Parameterwechsel verwirft
alte Resultate, Schließen markiert laufende Ergebnisse als abgebrochen.

Verifikation: zehn gezielte Tests einschließlich abgeschlossener/abgebrochener/
leerer/fehlgeschlagener Suche, späten Batches, neuem Lauf, Panel-/Ordner-/Moduswechsel
und bestehender de/en-Tastatur-/Fokusprüfung beim Öffnen eines Treffers. Echter
Browser: temporär 2.000 Demo-Treffer, Scrollen bis Treffer 1999, Resize auf 900×650,
sechs schnelle Wiederöffnungen und erneutes Öffnen nach vollständigem Unmount.
Immer 2.000 Treffer im Status und 22–23 gerenderte virtuelle Zeilen; Scrollposition
kehrt beim Wiederöffnen konsistent an den Anfang zurück. Toolbar, Alt+F7, Menü und
Escape geprüft; ein X-Schließen-Knopf existiert in diesem Dialog nicht. Temporäre
Demo-Daten anschließend entfernt.

Die separate Indexsuche besitzt eigene lokale Formular-/Ergebnisdaten; deren
Sitzungswiederherstellung ist als Folgeticket 261 erfasst.
