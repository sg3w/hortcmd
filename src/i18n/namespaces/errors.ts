// ============================================================
// Backend error messages (TICKET-036).
//
// The backend never sends a finished sentence. It sends a stable code
// and a few safe parameters; the wording lives here, in both languages,
// and is chosen when the error is *displayed* — so switching the
// language re-renders an error that is already on screen, and a server's
// own words never reach a dialog.
//
// One entry per code in `errors::code::ALL`. A test walks that list and
// fails on a missing one, which is what keeps this file in step with
// the backend rather than in step with whoever last remembered.
//
// The `{placeholders}` are the parameters the backend attaches; both
// languages must use the same ones (also checked by a test).
// ============================================================

import { ns } from "./ns";

export const errors = ns(
  {
    // --- filesystem ---
    "error.fs.notFound": "Nicht gefunden",
    "error.fs.permission": "Keine Berechtigung",
    "error.fs.exists": "„{name}“ existiert bereits",
    "error.fs.notADirectory": "Kein Verzeichnis",
    "error.fs.isADirectory": "Das ist ein Verzeichnis",
    "error.fs.readOnly": "Schreibgeschützt",
    "error.fs.noSpace": "Nicht genug Speicherplatz",
    "error.fs.inUse": "Wird gerade verwendet",
    "error.fs.io": "Lese-/Schreibfehler",
    "error.fs.invalidName": "Ungültiger Name",
    "error.fs.tooLarge": "Zu groß",

    // --- transfers ---
    "error.transfer.journalLimit": "Wiederherstellungsgrenze erreicht. Prüfe offene Aufträge oder teile den Auftrag in kleinere Gruppen.",
    "error.transfer.journalUnavailable": "Wiederherstellungsdaten nicht verfügbar. Schließe weitere App-Instanzen und prüfe den Konfigurationsordner. Vorhandene Daten bleiben erhalten.",
    "error.transfer.checksum": "Prüfsumme stimmt nicht — die Kopie ist beschädigt",
    "error.transfer.noStaging":
      "Ausfallsichere Übertragung wird an diesem Ort nicht unterstützt — in den Einstellungen unter „Dateioperationen“ abschalten",
    "error.transfer.commit": "Ziel konnte nicht ersetzt werden",
    "error.transfer.sourceGone": "Quelle liegt in einem anderen Speicher",
    "error.transfer.intoItself": "Quelle und Ziel sind identisch",

    // --- archives ---
    "error.archive.passwordRequired": "Das Archiv ist verschlüsselt",
    "error.archive.passwordWrong": "Falsches Passwort",
    "error.archive.unknownFormat": "Unbekanntes Archivformat",
    "error.archive.unsafeEntry": "Archiv enthält einen unsicheren Pfad oder Link",
    "error.archive.limit": "Sicherheitsgrenze für das Entpacken überschritten",
    "error.archive.conflict": "Archiv enthält kollidierende Namen oder Dateitypen",
    "error.archive.corrupt": "Archiv ist beschädigt oder nicht lesbar",

    // --- network ---
    "error.net.unreachable": "{host} ist nicht erreichbar",
    "error.net.timeout": "Zeitüberschreitung",
    "error.net.auth": "Anmeldung abgelehnt",
    "error.net.protocol": "Protokollfehler",
    "error.net.notConnected": "Nicht verbunden",
    "error.net.lost": "Verbindung zum Server verloren",
    "error.net.noProfile": "Verbindungsprofil nicht gefunden",
    "error.net.unsupportedProtocol": "Protokoll „{protocol}“ wird nicht unterstützt",
    "error.net.noShell": "Dieses Protokoll bietet keine Shell",
    "error.net.certificateChanged": "Das freigegebene Zertifikat oder das Verbindungsprofil wurde geändert. Prüfe die Freigabe in der Profilbearbeitung.",
    "error.net.certificateInvalid": "Die Datei enthält kein unterstütztes einzelnes Zertifikat.",
    "error.net.certificateExpired": "Das Serverzertifikat ist abgelaufen.",
    "error.net.certificateFuture": "Das Serverzertifikat ist noch nicht gültig.",
    "error.net.certificateHostname": "Das Zertifikat gilt nicht für diesen Hostnamen.",
    "error.net.certificateRevoked": "Das Serverzertifikat wurde widerrufen.",
    "error.net.tlsProtocol": "Die sichere TLS-Aushandlung wurde abgelehnt. Es erfolgt kein Wechsel zu einer unsicheren Verbindung.",
    "error.net.plaintextBlocked": "Unverschlüsselte Verbindung gesperrt. Erweiterte Freigabe und Bestätigung im Verbindungsprofil erforderlich.",
    "error.net.tls": "TLS-Verbindung abgelehnt (Zertifikat nicht vertrauenswürdig)",

    // --- credentials ---
    "error.cred.locked": "Die KeePass-Datei ist gesperrt",
    "error.cred.wrongPassword": "Falsches Masterpasswort oder falsche Schlüsseldatei",
    "error.cred.store": "Der Zugangsdaten-Speicher ist nicht verfügbar",
    "error.cred.notFound": "Zugangsdaten nicht gefunden",
    "error.cred.readOnlyFormat":
      "Die Datenbank liegt im alten Format KDBX 3 vor und kann nur gelesen werden — bitte in KeePassXC auf KDBX 4 aktualisieren",

    // --- security ---
    "error.security.hostKeyUnknown":
      "Host-Schlüssel von {host} ist unbekannt — bitte im Verbindungsmanager bestätigen",
    "error.security.hostKeyMismatch":
      "Host-Schlüssel von {host} hat sich geändert ({fingerprint}) — Verbindung abgelehnt",
    "error.security.blockedUrl": "Diese Adresse darf nicht geöffnet werden",

    // --- capability (TICKET-031) ---
    "error.capability.unsupported": "{scheme} unterstützt diese Aktion nicht",
    "error.capability.offline": "Keine aktive Verbindung",

    // --- validation ---
    "error.validation.badAddress": "Ungültige Adresse",
    "error.validation.unknownScheme": "Unbekanntes Adressschema",
    "error.validation.badInput": "Ungültige Eingabe",

    // --- cancellation (never shown as a failure) ---
    "error.cancelled": "Abgebrochen",

    // --- internal ---
    "error.internal.unexpected": "Unerwarteter Fehler",

    // --- the frame around a message ---
    /** A code this build does not know — a newer backend. */
    "error.unknown": "Unerwarteter Fehler ({code})",
    /** Appended so a report can be matched to a log entry. */
    "error.reference": "Referenz: {id}",
    "error.details": "Technische Details",
    "error.item": "{item}: {message}",

    // --- what the user can do ---
    "error.action.retry": "Erneut versuchen",
    "error.action.reconnect": "Neu verbinden",
    "error.action.authenticate": "Zugangsdaten prüfen",
    "error.action.confirmHostKey": "Host-Schlüssel bestätigen",
    "error.action.providePassword": "Passwort eingeben",
    "error.action.freeSpace": "Speicherplatz freigeben",
    "error.action.checkPermissions": "Berechtigungen prüfen",
    "error.action.unsupported": "Hier nicht möglich",
    "error.action.report": "Melden",
  },
  {
    // --- filesystem ---
    "error.fs.notFound": "Not found",
    "error.fs.permission": "No permission",
    "error.fs.exists": "“{name}” already exists",
    "error.fs.notADirectory": "Not a directory",
    "error.fs.isADirectory": "That is a directory",
    "error.fs.readOnly": "Read-only",
    "error.fs.noSpace": "Not enough space",
    "error.fs.inUse": "Currently in use",
    "error.fs.io": "Read/write error",
    "error.fs.invalidName": "Invalid name",
    "error.fs.tooLarge": "Too large",

    // --- transfers ---
    "error.transfer.journalLimit": "Recovery limit reached. Review pending jobs or split this job into smaller groups.",
    "error.transfer.journalUnavailable": "Recovery data unavailable. Close other app instances and check the configuration folder. Existing data is preserved.",
    "error.transfer.checksum": "Checksum mismatch — the copy is corrupted",
    "error.transfer.noStaging":
      "Failure-safe transfers are not supported at this location — turn them off under “File operations” in the settings",
    "error.transfer.commit": "The destination could not be replaced",
    "error.transfer.sourceGone": "The source is on a different store",
    "error.transfer.intoItself": "Source and destination are the same",

    // --- archives ---
    "error.archive.passwordRequired": "The archive is encrypted",
    "error.archive.passwordWrong": "Wrong password",
    "error.archive.unknownFormat": "Unknown archive format",
    "error.archive.unsafeEntry": "The archive contains an unsafe path or link",
    "error.archive.limit": "An extraction safety limit was exceeded",
    "error.archive.conflict": "The archive contains conflicting names or entry types",
    "error.archive.corrupt": "The archive is damaged or unreadable",

    // --- network ---
    "error.net.unreachable": "{host} is not reachable",
    "error.net.timeout": "Timed out",
    "error.net.auth": "Sign-in refused",
    "error.net.protocol": "Protocol error",
    "error.net.notConnected": "Not connected",
    "error.net.lost": "Connection to the server lost",
    "error.net.noProfile": "Connection profile not found",
    "error.net.unsupportedProtocol": "Protocol “{protocol}” is not supported",
    "error.net.noShell": "This protocol offers no shell",
    "error.net.certificateChanged": "The approved certificate or connection profile changed. Review approval in the profile editor.",
    "error.net.certificateInvalid": "The file does not contain a supported single certificate.",
    "error.net.certificateExpired": "The server certificate has expired.",
    "error.net.certificateFuture": "The server certificate is not valid yet.",
    "error.net.certificateHostname": "The certificate does not match this hostname.",
    "error.net.certificateRevoked": "The server certificate has been revoked.",
    "error.net.tlsProtocol": "Secure TLS negotiation was refused. No fallback to an insecure connection is allowed.",
    "error.net.plaintextBlocked": "Unencrypted connection blocked. Enable the advanced setting and acknowledge the risk in this profile.",
    "error.net.tls": "TLS connection refused (certificate not trusted)",

    // --- credentials ---
    "error.cred.locked": "The KeePass file is locked",
    "error.cred.wrongPassword": "Wrong master password or key file",
    "error.cred.store": "The credential store is not available",
    "error.cred.notFound": "Credentials not found",
    "error.cred.readOnlyFormat":
      "The database is in the older KDBX 3 format and can only be read — please upgrade it to KDBX 4 in KeePassXC",

    // --- security ---
    "error.security.hostKeyUnknown":
      "The host key of {host} is unknown — please confirm it in the connection manager",
    "error.security.hostKeyMismatch":
      "The host key of {host} has changed ({fingerprint}) — connection refused",
    "error.security.blockedUrl": "This address may not be opened",

    // --- capability (TICKET-031) ---
    "error.capability.unsupported": "{scheme} does not support this action",
    "error.capability.offline": "No active connection",

    // --- validation ---
    "error.validation.badAddress": "Invalid address",
    "error.validation.unknownScheme": "Unknown address scheme",
    "error.validation.badInput": "Invalid input",

    // --- cancellation (never shown as a failure) ---
    "error.cancelled": "Cancelled",

    // --- internal ---
    "error.internal.unexpected": "Unexpected error",

    // --- the frame around a message ---
    "error.unknown": "Unexpected error ({code})",
    "error.reference": "Reference: {id}",
    "error.details": "Technical details",
    "error.item": "{item}: {message}",

    // --- what the user can do ---
    "error.action.retry": "Try again",
    "error.action.reconnect": "Reconnect",
    "error.action.authenticate": "Check the credentials",
    "error.action.confirmHostKey": "Confirm the host key",
    "error.action.providePassword": "Enter the password",
    "error.action.freeSpace": "Free up space",
    "error.action.checkPermissions": "Check the permissions",
    "error.action.unsupported": "Not possible here",
    "error.action.report": "Report",
  },
);
