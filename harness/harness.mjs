#!/usr/bin/env node
// ============================================================
// The remote-protocol integration harness (TICKET-038).
//
// Provisions disposable SFTP/SSH, FTP, explicit and implicit FTPS, SMB
// and WebDAV (HTTP and HTTPS) servers, seeds every one of them with the
// same deterministic tree, runs the Rust conformance suite against them,
// and tears everything down — including after a failure, including after
// Ctrl+C.
//
//   node harness/harness.mjs run          # provision → test → tear down
//   node harness/harness.mjs run --keep   # leave the servers up
//   node harness/harness.mjs up           # provision only
//   node harness/harness.mjs down [id]    # tear down one run, or all
//   node harness/harness.mjs status
//
// Everything a run needs lives under harness/.run/<id>/ and is deleted
// with the run: passwords, a private CA, the SSH host key, the seeded
// data. Nothing is committed, nothing outlives the run, and nothing is
// published beyond 127.0.0.1.
// ============================================================

import { spawn, spawnSync } from "node:child_process";
import { createServer } from "node:net";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import { randomBytes } from "node:crypto";

const HARNESS_DIR = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HARNESS_DIR, "..");
const RUNS = join(HARNESS_DIR, ".run");
const COMPOSE = join(HARNESS_DIR, "compose.yml");

/** How much of the copy path a "large" file has to exercise. */
const LARGE_FILE_BYTES = 3 * 1024 * 1024; // 12 × the 256 KB default buffer

// ---------- small helpers ----------

const log = (msg) => console.log(`harness: ${msg}`);

function run(cmd, args, opts = {}) {
  const r = spawnSync(cmd, args, { encoding: "utf8", ...opts });
  if (r.error) throw r.error;
  return r;
}

function must(cmd, args, opts = {}) {
  const r = run(cmd, args, { stdio: "inherit", ...opts });
  if (r.status !== 0) {
    throw new Error(`${cmd} ${args.join(" ")} failed with ${r.status}`);
  }
  return r;
}

/**
 * Ports the harness publishes on.
 *
 * Deliberately *not* asked for with `listen(0)`. That hands back a port
 * from the operating system's ephemeral range — the same range it hands
 * to everything else that wants a socket — and between the moment the
 * harness picks one and the moment Docker binds it, seconds later, it
 * can be gone. The failure then arrives as "port is already allocated"
 * on one container out of seven, which is a confusing way to learn that
 * a test suite raced the kernel.
 *
 * 20000–45000 sits below the ephemeral range on every platform this
 * runs on, so the operating system will not give these away by itself.
 */
const PORT_MIN = 20000;
const PORT_MAX = 45000;

function portFree(port) {
  return new Promise((ok) => {
    const s = createServer();
    s.on("error", () => ok(false));
    s.listen(port, "127.0.0.1", () => s.close(() => ok(true)));
  });
}

/** `count` free ports, none of them equal to another. */
async function freePorts(count, taken) {
  const out = [];
  for (let attempt = 0; out.length < count && attempt < 500; attempt++) {
    const port = PORT_MIN + Math.floor(Math.random() * (PORT_MAX - PORT_MIN));
    if (taken.has(port)) continue;
    if (!(await portFree(port))) continue;
    taken.add(port);
    out.push(port);
  }
  if (out.length < count) throw new Error("no free ports found");
  return out;
}

/**
 * FTP passive mode hands the client a port number from a range, and the
 * range must be published one for one — so unlike the single ports these
 * have to be contiguous and known in advance.
 */
async function freeRange(size, taken) {
  for (let attempt = 0; attempt < 200; attempt++) {
    const base = PORT_MIN + Math.floor(Math.random() * (PORT_MAX - PORT_MIN - size));
    const ports = Array.from({ length: size }, (_, i) => base + i);
    if (ports.some((p) => taken.has(p))) continue;
    const free = await Promise.all(ports.map(portFree));
    if (!free.every(Boolean)) continue;
    for (const p of ports) taken.add(p);
    return [base, base + size - 1];
  }
  throw new Error("no free passive-mode port range found");
}

// ---------- the seeded tree ----------

/**
 * One tree, seeded identically into every server, so a difference in a
 * listing is a difference in the *provider* and not in the fixture.
 *
 * `only` marks a name that not every protocol can hold. SMB inherits
 * Windows' reserved characters and refuses a trailing dot or space;
 * pretending otherwise would produce a harness that fails for a reason
 * that is not a bug.
 */
const SEED = [
  { dir: "tree" },
  { dir: "tree/nested" },
  { dir: "tree/nested/deeper" },
  { file: "tree/empty.txt", bytes: 0 },
  { file: "tree/small.txt", text: "hortcmd harness\n" },
  { file: "tree/large.bin", size: LARGE_FILE_BYTES },
  { file: "tree/nested/deeper/leaf.txt", text: "leaf\n" },
  { file: "tree/name with spaces.txt", text: "spaces\n" },
  { file: "tree/.hidden", text: "dotfile\n" },
  { dir: "tree/.hidden-dir" },
  { file: "tree/.hidden-dir/inside.txt", text: "inside\n" },
  { file: "tree/Ümlaut-Grüße.txt", text: "unicode\n" },
  { file: "tree/日本語.txt", text: "unicode\n" },
  { file: "tree/emoji-🚀.txt", text: "unicode\n" },
  // Two names that differ only in case: one entry on a case-insensitive
  // store, two on a case-sensitive one. The suite asserts *consistency*,
  // not a count.
  { file: "tree/Case.txt", text: "upper\n" },
  { file: "tree/case.txt", text: "lower\n" },
  // Edge names. A dash, a leading dash (which a careless command line
  // would read as a flag) and a percent (which a careless URL builder
  // would eat) are safe everywhere; the Windows-reserved ones are not.
  { file: "tree/-leading-dash.txt", text: "dash\n" },
  { file: "tree/100%-done.txt", text: "percent\n" },
  { file: "tree/semi;colon.txt", text: "semicolon\n" },
  { file: "tree/hash#tag.txt", text: "hash\n" },
  { file: "tree/quote'single.txt", text: "quote\n" },
  { file: "tree/trailing.dot.", text: "dot\n", only: ["sftp", "ftp", "webdav"] },
  { file: "tree/star*name.txt", text: "star\n", only: ["sftp", "ftp"] },
  { file: "tree/colon:name.txt", text: "colon\n", only: ["sftp", "ftp"] },
];

/** Deterministic bytes — the same run twice produces the same file. */
function filler(size) {
  const buf = Buffer.allocUnsafe(size);
  for (let i = 0; i < size; i++) buf[i] = (i * 31 + (i >> 8) * 7) & 0xff;
  return buf;
}

function seedInto(root, protocol) {
  mkdirSync(root, { recursive: true });
  for (const item of SEED) {
    if (item.only && !item.only.includes(protocol)) continue;
    if (item.dir) {
      mkdirSync(join(root, item.dir), { recursive: true });
      continue;
    }
    const path = join(root, item.file);
    mkdirSync(dirname(path), { recursive: true });
    if (item.size !== undefined) writeFileSync(path, filler(item.size));
    else if (item.bytes === 0) writeFileSync(path, "");
    else writeFileSync(path, item.text);
  }
  // Where the conformance suite does its own writing, so a failure never
  // leaves rubble in the read-only fixture above.
  mkdirSync(join(root, "work"), { recursive: true });
}

/**
 * What the Rust side is told to expect — read back from the seeded
 * directory rather than derived from [`SEED`].
 *
 * The two are not the same thing. A case-insensitive host (macOS by
 * default) collapses `Case.txt` and `case.txt` into one file; another
 * host may refuse a name outright. A manifest that describes the wish
 * rather than the result would fail every provider for something no
 * provider did.
 */
function seedManifest(root) {
  const out = [];
  const walk = (dir, prefix) => {
    for (const name of readdirSync(dir).sort()) {
      const rel = prefix ? `${prefix}/${name}` : name;
      const st = statSync(join(dir, name));
      if (st.isDirectory()) {
        out.push({ path: rel, dir: true, size: 0 });
        walk(join(dir, name), rel);
      } else {
        out.push({ path: rel, dir: false, size: st.size });
      }
    }
  };
  walk(join(root, "tree"), "tree");
  return out;
}

// ---------- credentials and certificates ----------

/**
 * Generated per run, never committed, never reused. The password is
 * random so a leaked harness log is worth nothing, and the CA exists
 * only so the TLS services have something to present.
 */
function makeSecrets(dir, hostnames) {
  mkdirSync(dir, { recursive: true });
  const password = randomBytes(18).toString("base64url");
  writeFileSync(join(dir, "password"), password, { mode: 0o600 });

  const cnf = join(dir, "openssl.cnf");
  writeFileSync(
    cnf,
    [
      "[req]",
      "distinguished_name = dn",
      "prompt = no",
      "[dn]",
      "CN = hortcmd-harness",
      "[server]",
      "basicConstraints = CA:FALSE",
      "keyUsage = digitalSignature, keyEncipherment",
      "extendedKeyUsage = serverAuth",
      `subjectAltName = ${hostnames.map((h) => (/^[\d.]+$/.test(h) ? `IP:${h}` : `DNS:${h}`)).join(", ")}`,
      "",
    ].join("\n"),
  );

  const ossl = (args) => {
    const r = run("openssl", args, { cwd: dir });
    if (r.status !== 0) throw new Error(`openssl ${args[0]}: ${r.stderr}`);
  };

  // A private CA, so the harness can also produce a certificate the
  // client must *reject* without that being a broken-server failure.
  ossl(["genrsa", "-out", "ca.key", "2048"]);
  ossl([
    "req", "-x509", "-new", "-key", "ca.key", "-sha256", "-days", "2",
    "-subj", "/CN=hortcmd harness CA", "-out", "ca.crt",
  ]);

  const leaf = (name, cfgSection) => {
    ossl(["genrsa", "-out", `${name}.key`, "2048"]);
    ossl(["req", "-new", "-key", `${name}.key`, "-config", cnf, "-out", `${name}.csr`]);
    ossl([
      "x509", "-req", "-in", `${name}.csr`, "-CA", "ca.crt", "-CAkey", "ca.key",
      "-CAcreateserial", "-days", "2", "-sha256",
      "-extfile", cnf, "-extensions", cfgSection, "-out", `${name}.crt`,
    ]);
  };
  leaf("server", "server");

  // A second CA and leaf: same hostnames, different issuer. Pointing a
  // service at these is how the harness produces a certificate mismatch.
  ossl(["genrsa", "-out", "rogue-ca.key", "2048"]);
  ossl([
    "req", "-x509", "-new", "-key", "rogue-ca.key", "-sha256", "-days", "2",
    "-subj", "/CN=hortcmd rogue CA", "-out", "rogue-ca.crt",
  ]);

  // SSH host keys: the live one, plus a spare the host-key test rotates
  // in to prove a changed key is refused.
  for (const name of ["ssh_host_ed25519_key", "ssh_host_rotated_key"]) {
    const r = run("ssh-keygen", [
      "-q", "-t", "ed25519", "-N", "", "-C", "hortcmd-harness",
      "-f", join(dir, name),
    ]);
    if (r.status !== 0) throw new Error(`ssh-keygen: ${r.stderr}`);
  }

  return password;
}

// ---------- provisioning ----------

function composeEnv(run) {
  return {
    ...process.env,
    COMPOSE_PROJECT_NAME: run.project,
    RUN_DIR: run.dir,
    HARNESS_BASE_ALPINE: run.images.alpine,
    HARNESS_BASE_DEBIAN: run.images.debian,
    HARNESS_USER: run.user,
    HARNESS_PASSWORD: run.password,
    HARNESS_SHARE: run.share,
    PORT_SSH: String(run.services.sftp.port),
    PORT_FTP: String(run.services.ftp.port),
    PORT_FTPS: String(run.services.ftps.port),
    PORT_FTPSI: String(run.services.ftpsi.port),
    PORT_SMB: String(run.services.smb.port),
    PORT_WEBDAV: String(run.services.webdav.port),
    PORT_WEBDAVS: String(run.services.webdavs.port),
    PASV_MIN_FTP: String(run.services.ftp.pasv[0]),
    PASV_MAX_FTP: String(run.services.ftp.pasv[1]),
    PASV_MIN_FTPS: String(run.services.ftps.pasv[0]),
    PASV_MAX_FTPS: String(run.services.ftps.pasv[1]),
    PASV_MIN_FTPSI: String(run.services.ftpsi.pasv[0]),
    PASV_MAX_FTPSI: String(run.services.ftpsi.pasv[1]),
  };
}

function compose(info, args, opts = {}) {
  return run("docker", ["compose", "-f", COMPOSE, ...args], {
    cwd: HARNESS_DIR,
    env: composeEnv(info),
    ...opts,
  });
}

async function provision({ id = randomBytes(3).toString("hex") } = {}) {
  const dir = join(RUNS, id);
  if (existsSync(dir)) throw new Error(`run ${id} already exists`);

  const images = JSON.parse(readFileSync(join(HARNESS_DIR, "images.json"), "utf8"));
  const user = "hortcmd";
  const share = "data";

  log(`run ${id}: allocating ports`);
  const taken = new Set();
  const [sftp, ftp, ftps, ftpsi, smb, webdav, webdavs] = await freePorts(7, taken);
  const ftpPasv = await freeRange(6, taken);
  const ftpsPasv = await freeRange(6, taken);
  const ftpsiPasv = await freeRange(6, taken);

  mkdirSync(dir, { recursive: true });
  const password = makeSecrets(join(dir, "secrets"), ["127.0.0.1", "localhost"]);

  // Seed first: the manifest below is read back from what actually
  // landed on disk, so it describes this host and not an intention.
  // `confined` says whether the *server* keeps the account inside the
  // served tree. Every service here does except SSH: an SSH account is a
  // login account, a chroot would take the shell with it, and the
  // terminal is one of the things being tested. Where a server does not
  // confine, hortcmd cannot invent a confinement — it is an FTP client,
  // not a sandbox — so the traversal check asserts something different
  // there, and says so.
  const layout = {
    sftp: { container: "ssh", protocol: "sftp", tls: "ssh", port: sftp, root: "/data", confined: false },
    ftp: { container: "ftp", protocol: "ftp", tls: "none", port: ftp, pasv: ftpPasv, root: "/", confined: true },
    ftps: { container: "ftps", protocol: "ftp", tls: "explicit", port: ftps, pasv: ftpsPasv, root: "/", confined: true },
    ftpsi: { container: "ftpsi", protocol: "ftp", tls: "implicit", port: ftpsi, pasv: ftpsiPasv, root: "/", confined: true },
    smb: { container: "smb", protocol: "smb", tls: "smb", port: smb, share, root: `/${share}`, confined: true },
    webdav: { container: "webdav", protocol: "webdav", tls: "none", port: webdav, root: "/", confined: true },
    webdavs: { container: "webdavs", protocol: "webdav", tls: "https", port: webdavs, root: "/", confined: true },
  };

  log(`run ${id}: seeding ${Object.keys(layout).length} trees`);
  const services = {};
  for (const [name, svc] of Object.entries(layout)) {
    const data = join(dir, "data", name);
    seedInto(data, svc.protocol);
    services[name] = { ...svc, seed: seedManifest(data) };
  }

  const info = {
    id,
    project: `hortcmd-harness-${id}`,
    dir,
    images,
    user,
    password,
    share,
    caCert: join(dir, "secrets", "ca.crt"),
    startedAt: new Date().toISOString(),
    services,
  };

  writeFileSync(join(dir, "run.json"), JSON.stringify(info, null, 2));

  // Build with the output on screen — it is slow the first time and
  // silence would look like a hang — then start with it captured, so a
  // failure can be read rather than only shown.
  log(`run ${id}: building service images`);
  const built = compose(info, ["build"], { stdio: "inherit" });
  if (built.status !== 0) {
    teardown(info);
    throw new Error("the service images did not build");
  }

  log(`run ${id}: starting services`);
  const up = compose(info, ["up", "-d", "--wait"]);
  const upOutput = `${up.stdout || ""}${up.stderr || ""}`;
  if (up.status !== 0) {
    process.stderr.write(upOutput);
    collectLogs(info);
    teardown(info);
    const error = new Error("services did not come up — see the collected logs");
    error.composeOutput = upOutput;
    throw error;
  }

  log(`run ${id}: ready`);
  return info;
}

/**
 * [`provision`], with one concession to reality: a port can be taken
 * between the moment it is checked and the moment Docker binds it —
 * by another harness run, or by anything else on the machine. That is a
 * collision, not a failure, and the answer is different ports rather
 * than a red suite.
 */
async function provisionWithRetries(attempts = 3) {
  for (let attempt = 1; ; attempt++) {
    try {
      return await provision();
    } catch (e) {
      const collision = /port is already allocated|address already in use/i.test(
        e.composeOutput ?? "",
      );
      if (!collision || attempt >= attempts) throw e;
      log(`port collision — retrying with new ports (${attempt}/${attempts - 1})`);
    }
  }
}

function teardown(info) {
  log(`run ${info.id}: tearing down`);
  compose(info, ["down", "-v", "--remove-orphans", "-t", "5"], { stdio: "inherit" });
  rmSync(info.dir, { recursive: true, force: true });
}

/**
 * Service logs, with the run's own secrets scrubbed. A failure artifact
 * that carries a working password is worse than no artifact.
 */
function collectLogs(info) {
  const out = join(RUNS, `${info.id}-logs`);
  mkdirSync(out, { recursive: true });
  const secrets = [info.password];
  try {
    secrets.push(readFileSync(join(info.dir, "secrets", "ca.key"), "utf8"));
    secrets.push(readFileSync(join(info.dir, "secrets", "server.key"), "utf8"));
    secrets.push(readFileSync(join(info.dir, "secrets", "ssh_host_ed25519_key"), "utf8"));
  } catch {
    // A run that failed during provisioning may not have all of them.
  }
  const redact = (text) =>
    secrets
      .filter(Boolean)
      .reduce((acc, s) => acc.split(s).join("«redacted»"), text)
      // Anything that still looks like a key block goes too.
      .replace(/-----BEGIN [^-]+-----[\s\S]*?-----END [^-]+-----/g, "«redacted key»");

  // What each container is *doing* — an exited container with an empty
  // log says nothing on its own, and that is exactly when this matters.
  const ps = compose(info, ["ps", "-a", "--format", "{{.Service}}\t{{.State}}\t{{.Status}}"]);
  writeFileSync(join(out, "containers.txt"), redact(`${ps.stdout || ""}${ps.stderr || ""}`));

  for (const [name, svc] of Object.entries(info.services)) {
    const r = compose(info, ["logs", "--no-color", "--timestamps", svc.container]);
    writeFileSync(join(out, `${name}.log`), redact(`${r.stdout || ""}${r.stderr || ""}`));
  }
  writeFileSync(
    join(out, "run.json"),
    redact(JSON.stringify({ ...info, password: "«redacted»" }, null, 2)),
  );
  log(`logs written to ${out} (secrets redacted)`);
  return out;
}

// ---------- the test run ----------

function cargoTest(info, extra) {
  return new Promise((done) => {
    const args = [
      "test",
      "--manifest-path",
      join(REPO, "src-tauri", "Cargo.toml"),
      "--lib",
      "harness_",
      "--",
      "--ignored",
      "--nocapture",
      "--test-threads=1",
      ...extra,
    ];
    // Explicit test-only opt-in for this isolated run's plaintext fixtures.
    mkdirSync(join(info.dir, "config"), { recursive: true });
    writeFileSync(join(info.dir, "config", "transport-policy.json"), JSON.stringify({ allowPlaintext: true }));
    const child = spawn("cargo", args, {
      cwd: REPO,
      stdio: "inherit",
      env: {
        ...process.env,
        HORTCMD_HARNESS: join(info.dir, "run.json"),
        // The harness's own CA, so the TLS services can be reached
        // without touching the machine's trust store.
        HORTCMD_EXTRA_CA: info.caCert,
        // Keep a test run's host keys and credentials out of the
        // developer's real configuration.
        HORTCMD_CONFIG_DIR: join(info.dir, "config"),
      },
    });
    child.on("exit", (code) => done(code ?? 1));
  });
}

// ---------- commands ----------

function runsOnDisk() {
  if (!existsSync(RUNS)) return [];
  return readdirSync(RUNS)
    .filter((n) => existsSync(join(RUNS, n, "run.json")))
    .map((n) => JSON.parse(readFileSync(join(RUNS, n, "run.json"), "utf8")));
}

async function cmdRun(argv) {
  const keep = argv.includes("--keep");
  const extra = argv.filter((a) => !a.startsWith("--"));
  const info = await provisionWithRetries();

  // Ctrl+C must not leave seven containers and a private key behind.
  let downed = false;
  const bail = () => {
    if (downed) return;
    downed = true;
    teardown(info);
    process.exit(130);
  };
  process.on("SIGINT", bail);
  process.on("SIGTERM", bail);

  let code = 1;
  try {
    code = await cargoTest(info, extra);
  } finally {
    if (code !== 0) collectLogs(info);
    if (keep) {
      log(`run ${info.id}: kept — tear down with "node harness/harness.mjs down ${info.id}"`);
      log(`  config: ${join(info.dir, "run.json")}`);
    } else if (!downed) {
      downed = true;
      teardown(info);
    }
  }
  process.exit(code);
}

async function cmdUp() {
  const info = await provisionWithRetries();
  console.log(JSON.stringify({ id: info.id, run: join(info.dir, "run.json") }, null, 2));
}

function cmdDown(argv) {
  const wanted = argv.find((a) => !a.startsWith("--"));
  const all = runsOnDisk().filter((r) => !wanted || r.id === wanted);
  if (!all.length) {
    log(wanted ? `no run ${wanted}` : "no runs to tear down");
    return;
  }
  for (const info of all) teardown(info);
}

function cmdStatus() {
  const all = runsOnDisk();
  if (!all.length) return log("no runs");
  for (const info of all) {
    const ps = compose(info, ["ps", "--format", "{{.Service}} {{.State}}"]);
    console.log(`${info.id}  started ${info.startedAt}`);
    console.log(
      (ps.stdout || "")
        .trim()
        .split("\n")
        .filter(Boolean)
        .map((l) => `  ${l}`)
        .join("\n") || "  (no containers)",
    );
  }
}

const [command, ...argv] = process.argv.slice(2);
try {
  switch (command ?? "run") {
    case "run":
      await cmdRun(argv);
      break;
    case "up":
      await cmdUp();
      break;
    case "down":
      cmdDown(argv);
      break;
    case "status":
      cmdStatus();
      break;
    default:
      console.error(`harness: unknown command "${command}"`);
      console.error("  run [--keep] | up | down [id] | status");
      process.exit(2);
  }
} catch (e) {
  console.error(`harness: ${e.message}`);
  process.exit(1);
}
