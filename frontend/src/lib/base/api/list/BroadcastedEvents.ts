import type { ARUIAPI } from "../ARUIAPI";


export class BroadcastedEvents {

  handlers = {
    broadcastMessage: this.handleBroadcastMessage.bind(this),
  }

  constructor(public api: ARUIAPI) { }

  async init() {
    this.api.ipc.client.on("BroadcastMessage", this.handlers.broadcastMessage);
  }

  async destroy() {
    this.api.ipc.client.removeListener("BroadcastMessage", this.handlers.broadcastMessage);
  }

  handleBroadcastMessage(event: Electron.IpcRendererEvent, name: string, data: any) {
    // console.debug("BroadcastedEvents", "Received broadcast message:", name, data);
    this.api.events.emit(`BM:${name}`, data);
  }

  emit(name: string, data: any) {
    // console.debug("BroadcastedEvents", "Sending broadcast message:", name, data);
    this.api.ipc.client.send("BroadcastMessage", name, data);
  }

  on(name: string, callback: (data: any) => void) {
    this.api.events.on(`BM:${name}`, callback);
    return () => {
      this.api.events.off(`BM:${name}`, callback);
    }
  }

  off(name: string, callback: (data: any) => void) {
    this.api.events.off(`BM:${name}`, callback);
  }
}