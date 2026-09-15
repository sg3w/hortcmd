# TICKET-096: Netzwerkverbindungen überwachen und automatisch wiederherstellen

## Nutzerwunsch

Alle unterstützten Netzwerkverbindungen auf Abbrüche überwachen, durch geeignete
Keepalive- oder Zustandsprüfungen aktiv halten und nach einem Verbindungsverlust
kontrolliert wiederherstellen. Das umfasst SFTP/SSH, FTP, explizites und implizites
FTPS, SMB sowie WebDAV über HTTP/HTTPS. Neue Netzwerkprovider sollen sich über
dieselben Fähigkeiten integrieren können.

## Anforderungen

- Überwachung und Wiederverbindung im Rust-Backend implementieren; die bestehende
  Providersitzung, Verbindungsverwaltung und Statusereignisse weiterverwenden.
- Protokollspezifische, nicht verändernde Prüfungen verwenden: SSH-Keepalive für
  SFTP/SSH, geeignete NOOP-/Kontrollkanalprüfungen für FTP/FTPS, eine vom SMB-Client
  unterstützte Sitzungsprüfung und eine geeignete authentifizierte, lesende
  Anfrage für WebDAV. Die konkrete Methode anhand der Providerfähigkeiten
  festlegen und dokumentieren; fehlende Unterstützung ausdrücklich ausweisen.
- Bei zustandslosen oder gepoolten Verbindungen wie WebDAV die authentifizierte
  Erreichbarkeit des Dienstes prüfen, statt eine dauerhafte Sitzung vorzutäuschen.
  Prüfungen dürfen laufende Transfers nicht stören oder Protokollantworten anderer
  Operationen verbrauchen; Zugriffe auf gemeinsame Kontrollkanäle koordinieren.
- Ein ICMP-Ping oder eine separat erreichbare TCP-Verbindung genügt nicht als
  Nachweis, dass die verwendete Protokollsitzung oder Anmeldung noch funktioniert.
- Keepalive-Intervall, Antwort-Timeout und automatische Wiederverbindung
  pro Profil konfigurierbar machen; sinnvolle Standardwerte und eine Abschaltmöglichkeit
  vorsehen. Ausbleibende Antworten nach einer begrenzten Zahl von Versuchen als
  Verbindungsverlust behandeln.
- Wiederverbindungsversuche mit wachsendem Abstand, begrenzter Versuchszahl und
  Abbruchmöglichkeit ausführen. Pro Verbindung nur einen Versuch gleichzeitig
  zulassen; parallele Paneele und SSH-Terminals dürfen keine konkurrierenden
  Wiederverbindungen starten.
- Vorhandene Profile und den regulären Anmelde-/Hostkey-/TLS-Prüfpfad verwenden.
  Geänderte Hostkeys, abgelehnte TLS-Zertifikate, fehlende Zugangsdaten oder ein gesperrter Kennwortspeicher
  verlangen die bestehende Benutzerinteraktion statt endloser Wiederholungen.
  Keine Abschwächung der Transportprüfung oder automatische Protokoll-Downgrades.
- Absichtliches Trennen, Profillöschung und Programmende stoppen Keepalive und
  Wiederverbindung. Verspätete Antworten dürfen getrennte Sitzungen nicht
  reaktivieren. Standby und Netzwechsel berücksichtigen.
- Verbindungszustände verständlich anzeigen: verbunden, unterbrochen,
  Wiederverbindung läuft, endgültig fehlgeschlagen. Nach erfolgreicher Verbindung
  betroffene Verzeichnisansichten aktualisieren und ihren Pfad erhalten; nicht
  mehr verfügbare Pfade ausdrücklich melden.
- Laufende Kopier-/Verschiebeoperationen und interaktive Shell-Befehle nicht blind
  wiederholen. Wiederaufnahme nur über vorhandene geprüfte Recovery-Funktionen;
  unbekannten Ausführungszustand und erhaltene temporäre Dateien sichtbar machen.
  Ein abgebrochenes SSH-Terminal als beendet markieren; eine neue Shell benötigt
  eine eigene Sitzung und darf keine alten Eingaben erneut senden.

## Abnahme

- Für jeden unterstützten Netzwerkprovider deterministische Tests für
  antwortende/stumme/verzögerte Keepalive- oder Zustandsprüfungen,
  Verbindungsabbruch, erfolgreiche Wiederverbindung und erschöpfte Versuche.
- Tests für manuelles Trennen während eines Versuchs, geänderte Hostkeys/TLS-Zertifikate,
  fehlende Zugangsdaten, parallele Nutzer derselben Sitzung und verspätete
  Ergebnisse einer alten Sitzung.
- Unterbrochene Transfers behalten Quelle/Ziel gemäß bestehendem Sicherheits- und
  Recovery-Vertrag; weder Schreiboperationen noch Shell-Eingaben werden doppelt
  ausgeführt.
- Lokale Testserver bzw. der vorhandene Protokoll-Harness für SFTP, FTP, beide
  FTPS-Modi, SMB und WebDAV: Verbindungen unterbrechen und wiederherstellen,
  Prüfantworten aussetzen und Wiederverbindungsstatus prüfen. Eine Matrix
  dokumentiert pro Provider Prüfverfahren, Wiederverbindung und Testnachweis.
- `npm run check` besteht; Bedienung und Einstellungen sind dokumentiert.

## Einordnung

Direkte Weiterentwicklung aller unterstützten Netzwerkverbindungen. Baut auf TICKET-021 (Verbindungen),
TICKET-026 (SSH-Terminal), TICKET-032 (Verbindungslebenszyklus) und der bestehenden
Transfer-Recovery auf. Dieses Ticket ist offen; angelegt auf Nutzerwunsch.
