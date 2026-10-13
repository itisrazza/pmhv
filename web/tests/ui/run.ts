// Starts an isolated pmhvd, runs the UI tests against it, then force-stops any machines the tests
// left running and stops pmhvd. `just test-ui` builds everything first.

import { spawn, type ChildProcess } from "node:child_process";
import { once } from "node:events";
import * as fs from "node:fs/promises";
import { openSync } from "node:fs";
import * as net from "node:net";
import * as os from "node:os";
import * as path from "node:path";

const repo = path.resolve(import.meta.dirname, "../../..");
const binDir = process.env.PMHV_BIN_DIR ?? path.join(repo, "target", "debug");

async function freePort(): Promise<number> {
  const server = net.createServer();
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const address = server.address();
  server.close();
  if (address === null || typeof address === "string") {
    throw new Error(`unexpected server address: ${String(address)}`);
  }
  return address.port;
}

async function waitForHttp(url: string, daemon: ChildProcess): Promise<void> {
  const deadline = Date.now() + 15_000;
  while (Date.now() < deadline) {
    if (daemon.exitCode !== null) {
      throw new Error(`pmhvd exited early with status ${daemon.exitCode}`);
    }
    const response = await fetch(url).catch(() => null);
    if (response?.ok) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timed out waiting for pmhvd at ${url}`);
}

/** Asks each machine's shim to force-stop directly, so it works even if pmhvd has gone. */
async function forceStopAll(runtime: string): Promise<void> {
  const machines = await fs.readdir(path.join(runtime, "m")).catch(() => []);
  await Promise.all(
    machines.map(async (id) => {
      const socket = net.connect(path.join(runtime, "m", id, "ctl.sock"));
      socket.on("error", () => socket.destroy());
      socket.write('{"type":"power","action":"force_stop"}\n');
      await Promise.race([
        once(socket, "data"),
        once(socket, "close"),
        new Promise((resolve) => setTimeout(resolve, 5_000)),
      ]);
      socket.destroy();
    }),
  );
}

async function stop(child: ChildProcess): Promise<void> {
  if (child.exitCode !== null || child.signalCode !== null) {
    return;
  }
  const exited = once(child, "exit");
  child.kill("SIGTERM");
  await exited;
}

async function main(): Promise<number> {
  for (const binary of ["pmhvd", "pmhv", "pmhv-shim"]) {
    await fs.access(path.join(binDir, binary)).catch(() => {
      throw new Error(
        `${binary} not found in ${binDir}; build the workspace first (just test-ui does)`,
      );
    });
  }

  const data = await fs.mkdtemp(path.join(os.tmpdir(), "pmhv-ui-data-"));
  // Unix socket paths are limited to about 104 bytes on macOS, so this one stays short.
  const runtime = await fs.mkdtemp("/tmp/pmhv-ui-");
  const port = await freePort();
  await fs.writeFile(
    path.join(data, "config.toml"),
    `[http]\nlisten = "127.0.0.1:${port}"\n`,
  );
  const log = path.join(data, "pmhvd.log");
  const logFile = openSync(log, "w");
  const env = {
    ...process.env,
    PMHV_DATA_DIR: data,
    PMHV_RUNTIME_DIR: runtime,
  };
  const daemon = spawn(path.join(binDir, "pmhvd"), {
    env,
    stdio: ["ignore", logFile, logFile],
  });

  let tests: ChildProcess | undefined;
  let cleaning: Promise<void> | undefined;
  const tearDown = async (passed: boolean) => {
    if (tests) await stop(tests);
    await forceStopAll(runtime);
    await stop(daemon);
    if (!passed) {
      process.stderr.write(`\npmhvd log (${log}):\n`);
      process.stderr.write(await fs.readFile(log, "utf8"));
    }
    await fs.rm(runtime, { recursive: true, force: true });
    await fs.rm(data, { recursive: true, force: true });
  };
  // A signal and the normal exit path can both get here; tear down only once.
  const cleanup = (passed: boolean) => (cleaning ??= tearDown(passed));
  for (const signal of ["SIGINT", "SIGTERM"] as const) {
    process.once(signal, () => {
      void cleanup(true).finally(() => process.exit(130));
    });
  }

  let status = 1;
  try {
    const url = `http://127.0.0.1:${port}`;
    await waitForHttp(`${url}/api/v1/auth/session`, daemon);
    tests = spawn(
      process.execPath,
      [
        "--test",
        "--test-reporter=spec",
        path.join(import.meta.dirname, "ui.test.ts"),
      ],
      {
        env: { ...env, PMHV_UI_URL: url, PMHV_BIN_DIR: binDir },
        stdio: "inherit",
      },
    );
    const [code] = (await once(tests, "exit")) as [number | null];
    status = code ?? 1;
  } finally {
    await cleanup(status === 0);
  }
  return status;
}

process.exitCode = await main();
