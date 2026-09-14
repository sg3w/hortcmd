# TICKET-263: Native Prüfung der doppelten Vergleichsliste

Aus TICKET-064 ausgelagerte Plattformprüfung gemäß Nutzerwunsch:

- Windows/Linux: schmale/breite Fenster, beide Themes und Sprachen, Tastatur und
  native Screenreader mit gemeinsamem Tabellen-Scrollbereich prüfen.
- Echtes Host-Zoom von 200 % unter macOS/Windows/Linux prüfen. Der eingebettete
  Browser unterstützte den Host-Zoom-Shortcut nicht; 064 prüfte stattdessen den
  entsprechenden effektiven Platz von 640×360 Pixeln sowie normale Fenstergrößen.
- Windows: Selbstkopie über Hardlinks und andere Dateialiasse erkennen, bevor ein
  Ziel geöffnet wird. 064 prüft kanonische Pfade und unter Unix zusätzlich
  Geräte-/Inode-Identität. Windows-Dateiidentität gezielt ergänzen/verifizieren.
- Kopieren über unbekannte/unterbrochene Verbindungen muss weiterhin gesperrt
  bleiben. Dieser rekursive Vergleich unterstützt gegenwärtig lokale Ordner;
  keinen Provider-Support allein für den Test behaupten.

Befunde dokumentieren und npm run check vor Abschluss ausführen.
