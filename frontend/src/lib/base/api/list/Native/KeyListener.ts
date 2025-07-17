/* eslint-disable @typescript-eslint/no-require-imports */
const path = require("path") as typeof import("path");
const cp = require("child_process") as typeof import("child_process");
const JSONStream = require('json-stream');

import type { Native } from ".";

export class KeyListener {
  processes = new Map<string, import("child_process").ChildProcessWithoutNullStreams>();

  constructor(public native: Native) { }

  start(mode: "mouse" | "complex") {
    this.stop(mode);
    const exePath = path.join(this.native.api.ipc.getPath("appPath"), `./bins/key-listener.exe`);
    try {
      const jsonStream = new JSONStream();

      let process = cp.spawn(exePath, [mode], {
        cwd: path.dirname(exePath),
      });

      this.native.api.logger.info("KeyListener", "Key Listener started. for PID: " + process.pid, ` Mode: ${mode}`);

      process.stdout.setEncoding("utf-8");
      process.stdout.on("data", (data: any) => {
        jsonStream.write(data);
      });

      jsonStream.on("data", (data: any) => {
        this.native.api.events.emit("KeyListenerMessage", { data, mode });
      });

      process.once("error", (err) => {
        this.native.api.logger.error("KeyListener", `Key Listener error: ${err}`);
      });

      process.once("exit", () => {
        process?.removeAllListeners();
        this.processes.delete(mode);
      });

      this.processes.set(mode, process);
    } catch (e) {
      this.native.api.logger.error("KeyListener", `Failed to set up Key Listener stdout stream. ${e}`);
    }
  }

  async stopAll() {
    this.processes.forEach((process, key) => {
      this.stop(key as "mouse" | "complex");
    });
  }

  stop(mode: "mouse" | "complex") {
    const process = this.processes.get(mode);
    if (process) {
      this.native.api.logger.info("KeyListener", "Key Listener stopped for PID: " + process.pid, ` Mode: ${mode}`);
      process.removeAllListeners();
      process.kill();
      this.processes.delete(mode);
    }
  }

  async startAll() {
    this.start("mouse");
    this.start("complex");
  }

  async init() {
    this.startAll();
  }

  async destroy() {
    this.stopAll();
  }
}