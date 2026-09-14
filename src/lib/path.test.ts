// ============================================================
// Path handling, local and remote (TICKET-033).
//
// Every navigation, every join and every "am I at the top?" goes
// through these five functions, for Windows paths, POSIX paths and
// remote VFS addresses alike. A mistake here does not produce a visible
// error — it produces an operation against the wrong path, which is why
// the odd shapes (trailing slashes, spaces, non-ASCII, an authority
// that looks like a path) are pinned down here rather than left to
// manual testing.
// ============================================================

import { describe, expect, it } from "vitest";
import {
  ancestorChain,
  baseName,
  isRoot,
  joinPath,
  parentPath,
  rootOf,
  schemeOf,
} from "./path";

describe("schemeOf", () => {
  it("names the scheme of a remote address, lowercased", () => {
    expect(schemeOf("sftp://u@host/srv")).toBe("sftp");
    expect(schemeOf("SFTP://u@host/srv")).toBe("sftp");
    expect(schemeOf("webdav://h/x")).toBe("webdav");
  });

  it("is null for every local path", () => {
    expect(schemeOf("/Users/x")).toBeNull();
    expect(schemeOf("C:\\Users\\x")).toBeNull();
    expect(schemeOf("relative/dir")).toBeNull();
    // A colon alone is not a scheme — Windows drives must not be one.
    expect(schemeOf("C:/Users/x")).toBeNull();
  });
});

describe("joinPath", () => {
  it("uses the separator the base already uses", () => {
    expect(joinPath("/Users/x", "a.txt")).toBe("/Users/x/a.txt");
    expect(joinPath("C:\\Users\\x", "a.txt")).toBe("C:\\Users\\x\\a.txt");
    expect(joinPath("sftp://u@host/srv", "a.txt")).toBe(
      "sftp://u@host/srv/a.txt",
    );
  });

  it("does not double a separator the base ends with", () => {
    expect(joinPath("/Users/x/", "a.txt")).toBe("/Users/x/a.txt");
    expect(joinPath("C:\\", "a.txt")).toBe("C:\\a.txt");
    expect(joinPath("sftp://u@host/", "a.txt")).toBe("sftp://u@host/a.txt");
  });

  it("keeps spaces and non-ASCII names verbatim", () => {
    expect(joinPath("/Users/Jörg", "Meine Datei.txt")).toBe(
      "/Users/Jörg/Meine Datei.txt",
    );
    expect(joinPath("/срв", "日本語.txt")).toBe("/срв/日本語.txt");
  });
});

describe("parentPath", () => {
  it("walks up a POSIX path", () => {
    expect(parentPath("/Users/x/a.txt")).toBe("/Users/x");
    expect(parentPath("/Users/x")).toBe("/Users");
    expect(parentPath("/Users")).toBe("/");
    expect(parentPath("/")).toBe("/");
  });

  it("walks up a Windows path and stops at the drive", () => {
    expect(parentPath("C:\\Users\\x\\a.txt")).toBe("C:\\Users\\x");
    expect(parentPath("C:\\Users")).toBe("C:\\");
  });

  it("ignores trailing separators", () => {
    expect(parentPath("/Users/x/")).toBe("/Users");
    expect(parentPath("/Users/x///")).toBe("/Users");
  });

  it("never mangles a remote authority", () => {
    expect(parentPath("sftp://u@host:22/srv/data")).toBe("sftp://u@host:22/srv");
    expect(parentPath("sftp://u@host:22/srv")).toBe("sftp://u@host:22/");
    // Already at the connection root: stay there rather than eating the host.
    expect(parentPath("sftp://u@host:22/")).toBe("sftp://u@host:22/");
    expect(parentPath("sftp://u@host:22")).toBe("sftp://u@host:22/");
  });
});

describe("isRoot / rootOf", () => {
  it("recognizes the POSIX and Windows roots", () => {
    expect(isRoot("/")).toBe(true);
    expect(isRoot("C:\\")).toBe(true);
    expect(isRoot("C:")).toBe(true);
    expect(isRoot("/Users")).toBe(false);
    expect(rootOf("/Users/x")).toBe("/");
    expect(rootOf("C:\\Users\\x")).toBe("C:\\");
  });

  it("treats the connection root as the top of a remote address", () => {
    expect(isRoot("sftp://u@host/")).toBe(true);
    expect(isRoot("sftp://u@host")).toBe(true);
    expect(isRoot("sftp://u@host/srv")).toBe(false);
    expect(rootOf("sftp://u@host:22/srv/deep")).toBe("sftp://u@host:22/");
  });
});

describe("baseName", () => {
  it("takes the last component", () => {
    expect(baseName("/Users/x/a.txt")).toBe("a.txt");
    expect(baseName("C:\\Users\\x\\a.txt")).toBe("a.txt");
    expect(baseName("sftp://u@host/srv/a.txt")).toBe("a.txt");
  });

  it("ignores a trailing separator", () => {
    expect(baseName("/Users/x/")).toBe("x");
  });

  it("returns the root itself for a root", () => {
    expect(baseName("/")).toBe("/");
    expect(baseName("sftp://u@host/")).toBe("sftp://u@host/");
  });

  it("keeps spaces and non-ASCII", () => {
    expect(baseName("/a/Meine Datei.txt")).toBe("Meine Datei.txt");
    expect(baseName("/a/日本語")).toBe("日本語");
  });
});

describe("ancestorChain", () => {
  it("runs from the root down to the path", () => {
    expect(ancestorChain("/Users/x/deep")).toEqual([
      "/",
      "/Users",
      "/Users/x",
      "/Users/x/deep",
    ]);
  });

  it("stops at the connection root for a remote address", () => {
    expect(ancestorChain("sftp://u@host:22/srv/data")).toEqual([
      "sftp://u@host:22/",
      "sftp://u@host:22/srv",
      "sftp://u@host:22/srv/data",
    ]);
  });

  it("is a single element at a root", () => {
    expect(ancestorChain("/")).toEqual(["/"]);
    expect(ancestorChain("sftp://u@host/")).toEqual(["sftp://u@host/"]);
  });

  it("terminates on a Windows drive", () => {
    expect(ancestorChain("C:\\Users\\x")).toEqual([
      "C:\\",
      "C:\\Users",
      "C:\\Users\\x",
    ]);
  });
});
