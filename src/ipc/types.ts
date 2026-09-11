// ============================================================
// IPC types that are not generated (TICKET-035).
//
// Everything the Rust side owns comes from `bindings/` via ts-rs.
// These three are the frontend's own: `SearchOptions` is the shape the
// search dialog assembles and the command accepts, and the two unions
// name values the backend sends as plain strings.
// ============================================================

/** Which kind of operation a transfer is. */
export type TransferOp = "copy" | "move" | "extract" | "pack";

/** The four search modes the dialog offers. */
export type SearchMode = "files" | "duplicates" | "empty_dirs" | "large_files";

/** Everything one search run is parameterised by. */
export interface SearchOptions {
  mode: SearchMode;
  name: string;
  name_regex: boolean;
  content: string;
  content_regex: boolean;
  case_sensitive: boolean;
  ignore_dirs: string[];
  min_size: number;
}
