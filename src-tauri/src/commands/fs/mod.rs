// ============================================================
// Dateisystem-Commands, thematisch aufgeteilt:
//   dir     – Verzeichnisse/Laufwerke lesen
//   file    – Kopieren/Verschieben/Löschen/Ordner + Transfer-Infra
//   preview – Datei-Vorschau (F3) + Öffnen mit Standardprogramm (F4)
//   archive – ZIP: browsen, entpacken, packen
// ============================================================

pub mod archive;
pub mod compare;
pub mod dir;
pub mod filecompare;
pub mod file;
pub mod git;
pub mod preview;
pub mod props;
pub mod search;
pub mod tags;
pub mod watch;
