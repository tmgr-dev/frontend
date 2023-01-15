import { app, BrowserWindow, nativeTheme } from 'electron';
import path from 'path';

try {
	if (
		process.platform === 'win32' &&
		nativeTheme.shouldUseDarkColors === true
	) {
		require('fs').unlinkSync(
			require('path').join(app.getPath('userData'), 'DevTools Extensions'),
		);
	}
} catch (_) {}

let mainWindow;

function createWindow() {
	/**
	 * Initial window options
	 */
	mainWindow = new BrowserWindow({
		width: 1024,
		height: 1000,
		minWidth: 1024,
		minHeight: 768,
		useContentSize: true,
		frame: false,
		webPreferences: {
			enableRemoteModule: true,
			nodeIntegration: process.env.QUASAR_NODE_INTEGRATION,
			nodeIntegrationInWorker: process.env.QUASAR_NODE_INTEGRATION,
			contextIsolation: true,
			// More info: /quasar-cli/developing-electron-apps/electron-preload-script
			preload: path.resolve(__dirname, process.env.QUASAR_ELECTRON_PRELOAD),
		},
	});

	mainWindow.loadURL(process.env.APP_URL);

	if (process.env.DEBUGGING) {
		// if on DEV or Production with debug enabled
		mainWindow.webContents.openDevTools();
	} else {
		// we're on production; no access to devtools pls
		mainWindow.webContents.on('devtools-opened', () => {
			mainWindow.webContents.closeDevTools();
		});
	}

	mainWindow.on('closed', () => {
		mainWindow = null;
	});
}

app.on('ready', createWindow);

app.on('window-all-closed', () => {
	if (process.platform !== 'darwin') {
		app.quit();
	}
});

app.on('activate', () => {
	if (mainWindow === null) {
		createWindow();
	}
});
