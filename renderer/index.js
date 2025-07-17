const electron = require('electron');
const { app, BrowserWindow, ipcMain, dialog, screen } = electron;
const path = require('path');
const axios = require("axios");
const isDev = require('electron-is-dev');

process.env.ELECTRON_DISABLE_SECURITY_WARNINGS = 'true';

if (process.defaultApp) {
  if (process.argv.length >= 2) {
    app.setAsDefaultProtocolClient('arui', process.execPath, [path.resolve(process.argv[1])])
  }
} else {
  app.setAsDefaultProtocolClient('arui')
}
app.setAppUserModelId("rest.armagan.arui");

function createOverlayWindow(screenId) {
  const screens = screen.getAllDisplays();
  const screenInfo = screens.find(s => s.id === screenId);
  if (!screenInfo) {
    console.error(`Screen with ID ${screenId} not found.`);
    return null;
  }
  let w = new BrowserWindow({
    x: screenInfo.bounds.x,
    y: screenInfo.bounds.y,
    width: screenInfo.bounds.width,
    height: screenInfo.bounds.height,
    frame: false,
    transparent: true,
    backgroundColor: "#00000000",
    resizable: false,
    autoHideMenuBar: true,
    title: "ARUI Overlay",
    hasShadow: false,
    darkTheme: true,
    closable: false,
    skipTaskbar: true,
    webPreferences: {
      nodeIntegration: true,
      nodeIntegrationInSubFrames: true,
      nodeIntegrationInWorker: true,
      contextIsolation: false,
      backgroundThrottling: false,
      webSecurity: false
    },
  });
  w.setAlwaysOnTop(true, "screen-saver", 1);
  return w;
}

/** @type {Map<string, BrowserWindow>} */
const overlayWindows = new Map();

async function createApp() {
  const mainWindow = new BrowserWindow({
    width: 1200,
    height: 700,
    minWidth: 1200,
    minHeight: 700,
    frame: false,
    transparent: true,
    backgroundColor: "#00000000",
    resizable: true,
    autoHideMenuBar: true,
    center: true,
    title: "ARUI",
    hasShadow: false,
    darkTheme: true,
    webPreferences: {
      nodeIntegration: true,
      nodeIntegrationInSubFrames: true,
      nodeIntegrationInWorker: true,
      contextIsolation: false,
      backgroundThrottling: false,
      webSecurity: false
    },
  });

  mainWindow.webContents.openDevTools({ mode: 'detach' });

  ipcMain.on("BroadcastMessage", (event, ...args) => {
    [mainWindow, ...overlayWindows.values()].forEach(win => {
      if (win && !win.isDestroyed()) {
        win.webContents.send("BroadcastMessage", ...args);
      }
    });
  });

  ipcMain.handle("GetScreenInfo", () => {
    return screen.getAllDisplays();
  });

  ipcMain.handle("CreateOverlayWindow", (_, { screenId, id, path }) => {
    overlayWindows.get(id)?.destroy();
    const win = createOverlayWindow(screenId);
    if (!win) return;
    overlayWindows.set(id, win);
    if (path && path.startsWith("/")) path = path.slice(1);
    if (path && path.endsWith("/")) path = path.slice(0, -1);
    if (isDev) {
      win.loadURL(`http://localhost:5173/${path}`);
    } else {
      win.loadFile(`./build/${path}/index.html`.replace(/\/\//g, '/'));
    }
    win.show();
    win.openDevTools();
  });

  ipcMain.handle("DestroyOverlayWindow", (_, id) => {
    overlayWindows.get(id)?.destroy();
  });

  ipcMain.handle("KillAllOverlayWindows", () => {
    overlayWindows.forEach((win, id) => {
      win.destroy();
      overlayWindows.delete(id);
    });
  });

  ipcMain.handle("SetOverlayWindowIgnoreMouseEvents", async (_, id, ignore = true) => {
    const win = overlayWindows.get(id);
    if (!win || win.isDestroyed()) return;
    win.setIgnoreMouseEvents(ignore, { forward: true });
  });

  ipcMain.handle("SetOverlayWindowAlign", async (_, id, { width, height, x, y, visible } = {}) => {
    const win = overlayWindows.get(id);
    if (!win || win.isDestroyed()) return;
    if (width !== undefined && height !== undefined) {
      win.setResizable(true);
      win.setSize(width, height);
      win.setResizable(false);
    }
    if (x !== undefined && y !== undefined) {
      win.setPosition(x, y);
    }
    if (visible !== undefined) {
      const isVisible = win.isVisible();
      if (isVisible === visible) return;
      if (visible) {
        win.show();
      } else {
        win.hide();
      }
    }
  });

  ipcMain.handle("BringOverlayToFront", async (_, id) => {
    const win = overlayWindows.get(id);
    if (!win || win.isDestroyed()) return;
    win.setAlwaysOnTop(false);
    setTimeout(() => {
      win.setAlwaysOnTop(true, "screen-saver", 1);
    }, 50);
  });

  ipcMain.handle("Eval", async (_, code) => {
    try {
      return await eval(code);
    } catch (error) {
      return {
        error: error.message,
        stack: error.stack,
      };
    }
  });

  ipcMain.on("Quit", () => {
    overlayWindows.forEach((win, id) => {
      win.destroy();
      overlayWindows.delete(id);
    });
    mainWindow.destroy();
    setTimeout(() => {
      app.quit();
    }, 100); // Allow time for the windows to close before quitting
  });

  ipcMain.on("GetPath", (event, arg) => {
    if (arg === "appPath") {
      event.returnValue = app.getAppPath();
      return;
    }

    try {
      event.returnValue = app.getPath(arg);
    } catch {
      event.returnValue = null;
    }
  });

  ipcMain.on("GetAppName", (event, arg) => {
    event.returnValue = app.getName();
  });

  ipcMain.on("Relaunch", (event, arg) => {
    app.relaunch();
    app.exit();
  });

  ipcMain.handle("Fetch", async (event, reqData = {}) => {
    try {
      const response = await axios(reqData);
      return {
        data: response.data,
        status: response.status,
        statusText: response.statusText,
        headers: response.headers,
        url: response.request.res.responseUrl
      };
    } catch (error) {
      return {
        error: error.message,
        data: error.response?.data,
        status: error.response?.status,
        statusText: error.response?.statusText,
        headers: error.response?.headers,
        url: error.response?.request?.res?.responseUrl
      };
    }
  });

  ipcMain.handle("ShowDialog", async (
    event,
    {
      mode = "open",
      openDirectory = false,
      openFile = true,
      multiSelections = false,
      filters,
      promptToCreate = false,
      defaultPath,
      title,
      showOverwriteConfirmation,
      message,
      showHiddenFiles,
      modal = false,
      buttons,
      defaultId,
      type,
      cancelId
    } = {}
  ) => {
    const show = {
      open: dialog.showOpenDialog,
      save: dialog.showSaveDialog,
      message: dialog.showMessageBox,
    }[mode];
    if (!show) return { error: `Invalid mode.`, ok: false };

    return await show.apply(dialog, [
      modal && BrowserWindow.fromWebContents(event.sender),
      {
        defaultPath,
        filters,
        title,
        message,
        createDirectory: true,
        buttons,
        type,
        defaultId,
        cancelId,
        properties: [
          showHiddenFiles && "showHiddenFiles",
          openDirectory && "openDirectory",
          promptToCreate && "promptToCreate",
          openDirectory && "openDirectory",
          openFile && "openFile",
          multiSelections && "multiSelections",
          showOverwriteConfirmation && "showOverwriteConfirmation"
        ].filter(Boolean),
      }
    ].filter(Boolean))
  });

  ipcMain.on("GetAppVersion", async (e) => {
    e.returnValue = app.getVersion();
  });

  if (isDev) {
    mainWindow.loadURL('http://localhost:5173');
  } else {
    mainWindow.loadFile('./build/index.html');
  }
}

app.whenReady().then(async () => {
  if (!app.requestSingleInstanceLock()) {
    app.quit();
    return;
  }
  await new Promise(r => setTimeout(r, 300));
  createApp();

  app.on('activate', function () {
    if (BrowserWindow.getAllWindows().length === 0) createApp()
  })
});

app.on('window-all-closed', function () {
  if (process.platform !== 'darwin') app.quit();
});

app.on('second-instance', (event, argv) => {
  let win = BrowserWindow.getAllWindows()[0];
  if (!win) return;
  win.show();
  win.focus();

  let deepLink = argv.pop();
  if (deepLink && deepLink.startsWith("arui://")) {
    deepLink = deepLink.replace("arui://", "");
    if (deepLink.startsWith("/")) deepLink = deepLink.slice(1);
    if (deepLink.endsWith("/")) deepLink = deepLink.slice(0, -1);
    deepLink = decodeURIComponent(deepLink);
    win.webContents.send("DeepLink", deepLink);
  }
});