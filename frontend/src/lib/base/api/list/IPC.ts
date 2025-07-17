/* eslint-disable @typescript-eslint/no-require-imports */
const { ipcRenderer } = require("electron") as typeof import("electron");

import type { ARUIAPI } from "../ARUIAPI";

export class IPC {
  client = ipcRenderer;

  constructor(public api: ARUIAPI) { }

  getPath(name: "home" | "appData" | "userData" | "sessionData" | "temp" | "exe" | "module" | "desktop" | "documents" | "downloads" | "music" | "pictures" | "videos" | "recent" | "logs" | "crashDumps" | "appPath") {
    return this.client.sendSync("GetPath", name) as string;
  }
}