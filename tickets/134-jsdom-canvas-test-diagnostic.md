# TICKET-134: Fehlende Canvas-Unterstützung im Frontend-Testaufbau klären

## Befund
Nach der Warnungsbereinigung 224/272 meldet die erfolgreiche Frontend-Suite einmal:
`Not implemented: HTMLCanvasElement's getContext() method: without installing the canvas npm package`.
Alle 670 Tests bestehen. Auslöser noch nicht isoliert; nicht als Produktionsfehler
oder fehlgeschlagenen Test behandeln.

## Abnahme
Aufruf isolieren. Benötigte Canvas-Verwendung realistisch testen oder einen
passenden lokalen Testadapter einsetzen. Keine pauschale Konsolenunterdrückung
und keine Abschaltung relevanter Accessibility-Prüfungen. npm run check bestehen.
