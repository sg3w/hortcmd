import type { OperationPlan } from "@/ipc/bindings/OperationPlan";
import type { PreflightRequest } from "@/ipc/domains/preflight";
/** Small complete native response for tests concerned with the surrounding UI. */
export function preflightFixture(request: PreflightRequest): OperationPlan {
  return {
    version: 1, digest: "reviewed-plan", kind: request.kind, destination: { kind: "local", path: request.destination }, destinationGeneration: "local", destinationIdentity: "destination", settings: request.settings,
    mappings: request.sources.map(path => ({ source: { kind: "local", path }, target: { kind: "local", path: `${request.destination}/${path.split("/").pop()}` }, sourceGeneration: "local", sourceVersion: "source", targetVersion: null, targetExists: false, files: 1, bytes: 10, existingBytes: 0, nativeMove: false })),
    limits: { availableBytes: 1000, quotaRemainingBytes: null, maximumObjectBytes: null, maximumNameBytes: 255, maximumPathBytes: 1024, caseSensitive: null, writable: true, staging: true, reservedNames: [], forbiddenCharacters: null, preservesMetadata: null },
    space: { additionalBytes: 10, stagingBytes: 10, backupBytes: 0, peakFootprintBytes: 10, verificationReadBytes: 0 }, issues: [],
  };
}
