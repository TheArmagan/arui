/* eslint-disable @typescript-eslint/no-require-imports */
const path = require("path") as typeof import("path");
const cp = require("child_process") as typeof import("child_process");
const fs = require("original-fs") as typeof import("fs");
const util = require("util") as typeof import("util");
const JSONStream = require('json-stream');

const execAsync = util.promisify(cp.exec);

import type { Native } from ".";


export interface MediaState {
  title?: string;
  artist?: string;
  album?: string;
  playback_status: "Playing" | "Paused" | "Stopped" | "Unknown";
  position?: number;
  duration?: number;
  app_name?: string;
  has_artwork: boolean;
}

export class MediaInfo {
  artwork: string | null = $state(null);
  media = $state<MediaState | null>(null);

  exePath!: string;
  process: import("child_process").ChildProcessWithoutNullStreams | null = null;

  constructor(public native: Native) {
  }

  async init() {
    this.exePath = path.join(this.native.api.ipc.getPath("appPath"), `./bins/win-media-info/win-media-info.exe`);
    this.start();
  }

  async destroy() {
    this.stop();
  }

  async skipTrack() {
    await execAsync(`"${this.exePath}" skip-track`);
  }

  async previousTrack() {
    await execAsync(`"${this.exePath}" previous-track`);
  }

  async togglePlayPause() {
    await execAsync(`"${this.exePath}" toggle-play-pause`);
  }

  async pause() {
    await execAsync(`"${this.exePath}" pause`);
  }

  async resume() {
    await execAsync(`"${this.exePath}" resume`);
  }

  start() {
    this.stop();

    try {
      const jsonStream = new JSONStream();

      let process = cp.spawn(this.exePath, {
        cwd: path.dirname(this.exePath),
      });

      this.native.api.logger.info("MediaControls", "Media Listener started. for PID: " + process.pid);

      process.stdout.setEncoding("utf-8");
      process.stdout.on("data", (data: any) => {
        jsonStream.write(data);
      });

      jsonStream.on("data", async (data: any) => {
        let oldKey = `${this.media?.title}-${this.media?.artist}-${this.media?.album}-${this.media?.app_name}`;
        let newKey = `${data.title}-${data.artist}-${data.album}-${data.app_name}`;
        this.media = data as MediaState;
        try {
          if (oldKey !== newKey) {
            this.artwork = await fs.promises.readFile(path.join(path.dirname(this.exePath), 'current_album_artwork.png'), "base64")
          }
        } catch (e) {
          this.native.api.logger.error("MediaControls", `Failed to read album artwork: ${e}`);
          this.artwork = null;
        }
        this.native.api.events.emit("MediaInfoMessage", data);
      });

      process.once("error", (err) => {
        this.native.api.logger.error("MediaControls", `Listener error: ${err}`);
      });

      process.once("exit", () => {
        process?.removeAllListeners();
        this.process = null;
      });

      this.process = process;
    } catch (e) {
      this.native.api.logger.error("MediaControls", `Failed to set up Tastkbar Item Listener stdout stream. ${e}`);
    }
  }

  stop() {
    if (this.process) {
      this.native.api.logger.info("MediaControls", "Stopping TaskbarItemList process");
      this.process.removeAllListeners();
      this.process.kill();
      this.process = null;
    }
  }
}