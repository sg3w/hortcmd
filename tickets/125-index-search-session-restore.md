# TICKET-125: Indexsuche nach Dialog-Unmount konsistent wiederherstellen

Bei der Prüfung von TICKET-062 aufgefallen: `SearchIndexPanel` speichert
Suchfelder, Scope-Auswahl und Ergebnisse lokal und verliert diese beim Unmount.
Die Live-Suche besitzt jetzt einen Sitzungsspeicher; die separate Indexsuche
benötigt eine eigene Lösung unter Beachtung gesperrter/entsperrter Index-Scopes.

- Wiederöffnen im gleichen erlaubten Scope stellt Parameter und Ergebnisstatus
  während der Sitzung wieder her, ohne automatisch erneut zu suchen.
- Scope-Wechsel, Sperren und Entfernen invalidieren zugehörige Ergebnisse.
- Passwörter nicht in den Sitzungsspeicher übernehmen.
- Späte Antworten nach Schließen oder Scope-Wechsel dürfen nichts überschreiben.
- Tests für Wiederöffnen, Abbruch und gesperrte Scopes ergänzen; npm run check.
