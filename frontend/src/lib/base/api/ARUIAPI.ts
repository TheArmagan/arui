/* eslint-disable @typescript-eslint/no-require-imports */
const { shell } = require("electron") as typeof import("electron");

import { BasicEventEmitter } from "./list/BasicEventEmitter";
import { BroadcastedEvents } from "./list/BroadcastedEvents";
import { IPC } from "./list/IPC";
import { Logger } from "./list/Logger";
import { Native } from "./list/Native";

export class ARUIAPI {
  shell = shell;
  ipc = new IPC(this);
  events = new BasicEventEmitter();
  broadcastedEvents = new BroadcastedEvents(this);
  logger = new Logger(this);
  native = new Native(this);
  constructor() { }

  async init() {
    this.logger.info("ARUIAPI", "Initializing ARUIAPI");
    // Initialize other components or services as needed
    await this.native.init();
    await this.broadcastedEvents.init();
    this.logger.info("ARUIAPI", "ARUIAPI initialized successfully");
  }

  async destroy() {
    this.logger.info("ARUIAPI", "Destroying ARUIAPI");
    // Clean up resources, listeners, etc.
    await this.native.destroy();
    await this.broadcastedEvents.destroy();
    this.logger.info("ARUIAPI", "ARUIAPI destroyed successfully");
  }
}