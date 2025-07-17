/* eslint-disable @typescript-eslint/no-require-imports */
const { ipcRenderer } = require("electron") as typeof import("electron");

import type { ARUIAPI } from "../ARUIAPI";

export class IPC {
  client = ipcRenderer;

  constructor(public api: ARUIAPI) { }

  getPath(name: "home" | "appData" | "userData" | "sessionData" | "temp" | "exe" | "module" | "desktop" | "documents" | "downloads" | "music" | "pictures" | "videos" | "recent" | "logs" | "crashDumps" | "appPath") {
    return this.client.sendSync("GetPath", name) as string;
  }

  getScreens() {
    return this.client.sendSync("GetScreens") as Electron.Display[];
  }

  async createOverlayWindow({ screenId, id, path }: { screenId: number, id: string, path: string }) {
    return await this.client.invoke("CreateOverlayWindow", { screenId, id, path });
  }

  async destroyOverlayWindow(id: string) {
    return await this.client.invoke("DestroyOverlayWindow", id);
  }

  async killAllOverlayWindows() {
    return await this.client.invoke("KillAllOverlayWindows");
  }

  async setOverlayWindowIgnoreMouseEvents(id: string, ignore: boolean) {
    return await this.client.invoke("SetOverlayWindowIgnoreMouseEvents", id, ignore);
  }

  async bringOverlayWindowToFront(id: string) {
    return await this.client.invoke("BringOverlayWindowToFront", id);
  }
}