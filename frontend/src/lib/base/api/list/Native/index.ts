import type { ARUIAPI } from "$lib/base/api/ARUIAPI";
import { KeyListener } from "./KeyListener.svelte";
import { MediaInfo } from "./MediaInfo.svelte";
import { TaskbarItemList } from "./TaskbarItemList.svelte";

export class Native {
  keyListener = new KeyListener(this);
  taskbarItemList = new TaskbarItemList(this);
  mediaInfo = new MediaInfo(this);

  constructor(public api: ARUIAPI) { }

  async init() {
    this.api.logger.info("Native", "Initializing Native API");
    // Initialize other components or services as needed
    await this.keyListener.init();
    await this.taskbarItemList.init();
    await this.mediaInfo.init();
    this.api.logger.info("Native", "Native API initialized successfully");
  }

  async destroy() {
    this.api.logger.info("Native", "Destroying Native API");
    // Clean up resources, listeners, etc.
    await this.keyListener.destroy();
    await this.taskbarItemList.destroy();
    await this.mediaInfo.destroy();
    this.api.logger.info("Native", "Native API destroyed successfully");
  }
}