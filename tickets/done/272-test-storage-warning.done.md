# TICKET-272: Node-localStorage-Warnung im Testaufbau vermeiden

## Befund
Nach der Korrektur von TICKET-224 bleibt pro Testworker eine ExperimentalWarning
zu fehlendem --localstorage-file. installStorage liest zuerst den nativen Getter,
obwohl anschließend ohnehin MemoryStorage eingesetzt wird.

## Umsetzung und Abnahme
Den Property-Deskriptor prüfen, ohne den nativen Getter aufzurufen. Bestehenden
MemoryStorage wiederverwenden; weiterhin isolierten Speicher pro Test verwenden.
Keine globale Warnungsunterdrückung und keine echte Datei für Testdaten anlegen.
Frontend-Suite und npm run check müssen ohne diese Warnung bestehen.

## Abschluss
- MemoryStorage-Erkennung über Property-Deskriptor statt nativen Getter.
- 670 Frontend-Tests bestehen ohne ExperimentalWarning oder Mock-Hoisting-Warnung.
- Vollständige Abnahme: npm run check.
- Eine separate jsdom-Canvas-Diagnose ist in TICKET-134 erfasst.
