# TICKET-126: Archiv-Orte im Navigationsverlauf erhalten

Bei TICKET-063 festgestellt: `loadArchive` schreibt bisher keinen Verlauf;
`goBack`/`goForward` rufen ausschließlich `loadDir` mit einem String auf.
Daher funktioniert Zurück/Vor für lokale und Remote-Verzeichnisse, kann aber
Archiv-Container und Innenpfad nicht wiederherstellen. Dies betrifft auch
normales Öffnen von Archiven und ist unabhängig vom neuen Gegenseiten-Befehl.

- Verlaufseinträge als vollständige Ortsidentität einschließlich Archiv speichern.
- Zurück/Vor zwischen Verzeichnissen, Archivwurzel und Unterordnern prüfen.
- Session-Migration bestehender String-Verläufe und Passwortabfragen berücksichtigen.
- Navigation bei Fehlern und verspäteten Antworten nicht verfälschen.
- Regressionsfälle für normales und gegenüberliegendes Öffnen; npm run check.
