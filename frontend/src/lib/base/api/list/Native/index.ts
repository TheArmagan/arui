import type { ARUIAPI } from "$lib/base/api/ARUIAPI";
import { KeyListener } from "./KeyListener";

export class Native {
  keyListener = new KeyListener(this);
  constructor(public api: ARUIAPI) { }

  async init() {
    this.api.logger.info("Native", "Initializing Native API");
    // Initialize other components or services as needed
    await this.keyListener.init();
    this.api.logger.info("Native", "Native API initialized successfully");
  }

  async destroy() {
    this.api.logger.info("Native", "Destroying Native API");
    // Clean up resources, listeners, etc.
    await this.keyListener.destroy();
    this.api.logger.info("Native", "Native API destroyed successfully");
  }
}