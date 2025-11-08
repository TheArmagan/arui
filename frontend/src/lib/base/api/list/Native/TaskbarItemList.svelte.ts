/* eslint-disable @typescript-eslint/no-explicit-any */
/* eslint-disable @typescript-eslint/no-require-imports */
const path = require("path") as typeof import("path");
const cp = require("child_process") as typeof import("child_process");
const util = require("util") as typeof import("util");
const JSONStream = require('json-stream');

const execAsync = util.promisify(cp.exec);

import type { Native } from "."

export interface TaskbarItem {
  title: string
  process_name: string
  process_id: number
  hwnd: number
  is_visible: boolean
  is_minimized: boolean
  is_maximized: boolean
  class_name: string
  has_taskbar_button: boolean
  window_state: string
  is_pinned: boolean
  executable_path: string
  item_type: string
  is_tray_icon: boolean
  is_definitely_taskbar: boolean
  is_definitely_tray: boolean
  is_system_window: boolean
  display_location: string
  is_focused: boolean
  is_running: boolean
}

export class TaskbarItemList {
  items = $state<TaskbarItem[]>([]);
  focusedItem = $derived(this.items.find(item => item.is_focused) || null);
  taskbarItems = $derived(this.items.filter(item => item.is_definitely_taskbar && item.title && item.hwnd));
  taskbarItemsGrouped = $derived(Object.values(Object.groupBy(this.taskbarItems, item => item.process_id))) as TaskbarItem[][];
  trayItems = $derived(this.items.filter(item => item.is_definitely_tray));

  icons: Record<string, string | null> = $state({});
  screenshots: Record<number, { at: number, data: string, width: number, height: number }> = $state({});

  exePath!: string;
  process: import("child_process").ChildProcessWithoutNullStreams | null = null;

  checkInterval: NodeJS.Timeout | null = null;
  constructor(public native: Native) {
  }

  async init() {
    this.exePath = path.join(this.native.api.ipc.getPath("appPath"), `./bins/win-taskbar-item-list.exe`);
    this.start();
    this.checkInterval = setInterval(() => {
      Object.entries(this.screenshots).forEach(([hwnd, data]) => {
        if (Date.now() - data.at > 60000 * 15) {
          delete this.screenshots[Number(hwnd)];
        }
      });
    }, 1000);
  }

  async destroy() {
    this.stop();
    if (this.checkInterval) {
      clearInterval(this.checkInterval);
      this.checkInterval = null;
    }
  }

  async getExecutableImage(exePath: string, force: boolean = false): Promise<string | null> {
    if (this.icons[exePath] && !force) {
      return this.icons[exePath];
    }

    const res = await execAsync(`"${this.exePath}" get-executable-icon --path "${exePath}"`)
    res.stdout = res.stdout.trim();
    const json = JSON.parse(res.stdout);
    if (!json.success) {
      delete this.icons[exePath];
      return null;
    }

    this.icons[exePath] = json.icon_base64 as string;
    return json.icon_base64 as string;
  }

  async getWindowScreenshot(hwnd: number, force: boolean = false): Promise<string | null> {
    // HWND 0 olan itemlar için screenshot alınamaz
    if (hwnd === 0) {
      return null;
    }

    if (this.screenshots[hwnd] && !force && (Date.now() - this.screenshots[hwnd].at < 15000)) {
      return this.screenshots[hwnd].data;
    }

    const res = await execAsync(`"${this.exePath}" get-window-screenshot --hwnd ${hwnd} --size 256x256`);
    res.stdout = res.stdout.trim();
    const json = JSON.parse(res.stdout);
    if (!json.success) {
      delete this.screenshots[hwnd];
      return null;
    }

    if (json.width < 50 || json.height < 50) {
      return null;
    }

    this.screenshots[hwnd] = {
      at: Date.now(),
      data: json.screenshot_base64 as string,
      width: json.width,
      height: json.height
    };

    return json.screenshot_base64 as string;
  }

  async openStartMenu() {
    await execAsync(`"${this.exePath}" open-start-menu`);
  }
  async minimizeWindow(hwnd: number) {
    await execAsync(`"${this.exePath}" minimize-window --hwnd ${hwnd}`);
  }

  async maximizeWindow(hwnd: number) {
    await execAsync(`"${this.exePath}" maximize-window --hwnd ${hwnd}`);
  }

  async restoreWindow(hwnd: number) {
    await execAsync(`"${this.exePath}" restore-window --hwnd ${hwnd}`);
  }

  async closeWindow(hwnd: number) {
    await execAsync(`"${this.exePath}" close-window --hwnd ${hwnd}`);
  }

  async focusWindow(hwnd: number) {
    await execAsync(`"${this.exePath}" focus-window --hwnd ${hwnd}`);
  }

  async unfocusWindow(hwnd: number) {
    await execAsync(`"${this.exePath}" unfocus-window --hwnd ${hwnd}`);
  }

  async toggleFocusWindow(hwnd: number) {
    await execAsync(`"${this.exePath}" toggle-focus-window --hwnd ${hwnd}`);
  }

  async startExecutable(exePath: string) {
    await execAsync(`"${this.exePath}" start-executable --path "${exePath}"`);
  }

  start() {
    this.stop();

    try {
      const jsonStream = new JSONStream();

      const process = cp.spawn(this.exePath, {
        cwd: path.dirname(this.exePath),
      });

      this.native.api.logger.info("TaskbarItemList", "Item Listener started. for PID: " + process.pid);

      process.stdout.setEncoding("utf-8");
      process.stdout.on("data", (data: any) => {
        jsonStream.write(data);
      });

      jsonStream.on("data", (data: any) => {
        this.native.api.events.emit("TastkbarItemMessage", data);
        if (data.action === "list") {
          this.items = data.items as TaskbarItem[];
          this.items.forEach((item) => {
            if (item.is_definitely_taskbar || item.is_definitely_tray || item.is_focused) {
              this.getExecutableImage(item.executable_path);
            }
            // Sadece HWND'si 0'dan farklı olan taskbar itemları için screenshot al
            if (item.is_definitely_taskbar && item.hwnd !== 0) {
              this.getWindowScreenshot(item.hwnd);
            }
          });
        }
      }); process.once("error", (err) => {
        this.native.api.logger.error("TastkbarItemMessage", `Listener error: ${err}`);
      });

      process.once("exit", () => {
        process?.removeAllListeners();
        this.process = null;
      });

      this.process = process;
    } catch (e) {
      this.native.api.logger.error("TastkbarItemMessage", `Failed to set up Tastkbar Item Listener stdout stream. ${e}`);
    }
  }

  stop() {
    if (this.process) {
      this.native.api.logger.info("TaskbarItemList", "Stopping TaskbarItemList process");
      this.process.removeAllListeners();
      this.process.kill();
      this.process = null;
    }
  }
}