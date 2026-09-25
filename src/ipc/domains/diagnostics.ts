import { invoke } from "../invoke";
import type { DiagnosticStatus } from "../bindings/DiagnosticStatus";
import type { DiagnosticLevel } from "../bindings/DiagnosticLevel";
export const diagnosticsStatus = () => invoke<DiagnosticStatus>("diagnostics_status");
export const diagnosticsLevel = (level: DiagnosticLevel) => invoke<void>("diagnostics_level", { level });
export const diagnosticsClear = () => invoke<void>("diagnostics_clear");
export const diagnosticsExport = (destination: string, includeLocation: boolean) => invoke<void>("diagnostics_export", { destination, includeLocation });
