import { describe, expect, it } from "vitest";
import { fillParams } from "./appError";

describe("single-pass error parameters", () => {
  it("preserves placeholder syntax in filenames regardless of parameter order", () => {
    for (const params of [
      { item: "{message}.txt", message: "No permission" },
      { message: "No permission", item: "{message}.txt" },
    ]) {
      expect(fillParams("{item}: {message}", params)).toBe("{message}.txt: No permission");
    }
  });

  it("replaces repeated placeholders and treats replacement syntax literally", () => {
    expect(fillParams("{item} / {item}: {message}", {
      item: "$&$1{message}", message: "{item}",
    })).toBe("$&$1{message} / $&$1{message}: {item}");
  });

  it("keeps missing parameters visible and supports empty values and nonword keys", () => {
    expect(fillParams("{missing}:{unset}:{empty}:{file-name}", {
      unset: undefined, empty: "", "file-name": "a.txt",
    })).toBe("{missing}:{unset}::a.txt");
    expect(fillParams("{constructor}:{toString}", {})).toBe("{constructor}:{toString}");
  });
});
