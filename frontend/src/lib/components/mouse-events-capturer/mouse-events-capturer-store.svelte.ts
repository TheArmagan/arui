import { api } from "@/base/api";

type MouseCapturerInstance = {
  id: string;
  overlayId: string;
  isMouseInside: boolean;
  container: HTMLDivElement;
  onMouseEvent?: (o: {
    type: 'enter' | 'leave' | 'move';
    preventDefault(): void;
    doDefault(): void;
    target: HTMLDivElement;
  }) => void;
};

class MouseEventsCapturer {
  private instances = $state<Map<string, MouseCapturerInstance>>(new Map());
  private leaveTimeout: number | null = null;
  private readonly DEBOUNCE_MS = 50;

  register(instance: MouseCapturerInstance) {
    this.instances.set(instance.id, instance);
    return () => this.unregister(instance.id);
  }

  unregister(id: string) {
    const instance = this.instances.get(id);
    if (instance?.isMouseInside) {
      this.handleLeave(id, true);
    }
    this.instances.delete(id);
  }

  updateMousePosition(id: string, isInside: boolean) {
    const instance = this.instances.get(id);
    if (!instance) return;

    if (isInside && !instance.isMouseInside) {
      // Mouse entered this instance
      this.handleEnter(id);
    } else if (!isInside && instance.isMouseInside) {
      // Mouse left this instance
      this.handleLeave(id);
    }
  }

  private handleEnter(id: string) {
    // Clear any pending leave timeout
    if (this.leaveTimeout !== null) {
      clearTimeout(this.leaveTimeout);
      this.leaveTimeout = null;
    }

    const instance = this.instances.get(id);
    if (!instance) return;

    instance.isMouseInside = true;

    let preventDefault = false;
    instance.onMouseEvent?.({
      type: 'enter',
      preventDefault: () => (preventDefault = true),
      doDefault: () => {
        if (api?.ipc?.setOverlayWindowIgnoreMouseEvents) {
          api.ipc.setOverlayWindowIgnoreMouseEvents(instance.overlayId, false);
        }
      },
      target: instance.container
    });

    if (!preventDefault) {
      if (api?.ipc?.setOverlayWindowIgnoreMouseEvents) {
        api.ipc.setOverlayWindowIgnoreMouseEvents(instance.overlayId, false);
      }
    }
  }

  private handleLeave(id: string, immediate = false) {
    const instance = this.instances.get(id);
    if (!instance) return;

    instance.isMouseInside = false;

    // Clear any existing timeout
    if (this.leaveTimeout !== null) {
      clearTimeout(this.leaveTimeout);
      this.leaveTimeout = null;
    }

    const executeLeave = () => {
      // Check if mouse is not in any other instance
      const hasMouseInAnyInstance = Array.from(this.instances.values()).some(
        (inst) => inst.isMouseInside
      );

      if (!hasMouseInAnyInstance) {
        let preventDefault = false;
        instance.onMouseEvent?.({
          type: 'leave',
          preventDefault: () => (preventDefault = true),
          doDefault: () => {
            if (api?.ipc?.setOverlayWindowIgnoreMouseEvents) {
              api.ipc.setOverlayWindowIgnoreMouseEvents(instance.overlayId, true);
            }
          },
          target: instance.container
        });

        if (!preventDefault) {
          if (api?.ipc?.setOverlayWindowIgnoreMouseEvents) {
            api.ipc.setOverlayWindowIgnoreMouseEvents(instance.overlayId, true);
          }
        }
      }

      this.leaveTimeout = null;
    };

    if (immediate) {
      executeLeave();
    } else {
      this.leaveTimeout = window.setTimeout(executeLeave, this.DEBOUNCE_MS);
    }
  }

  handleMove(id: string) {
    const instance = this.instances.get(id);
    if (!instance) return;

    instance.onMouseEvent?.({
      type: 'move',
      preventDefault: () => { },
      doDefault: () => { },
      target: instance.container
    });
  }
}

export const mouseEventsCapturer = new MouseEventsCapturer();
