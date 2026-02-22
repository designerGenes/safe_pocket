"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode = __importStar(require("vscode"));
const child_process_1 = require("child_process");
const path = __importStar(require("path"));
const os = __importStar(require("os"));
let statusBarItem;
let syncInProgress = false;
function getSpocketDir() {
    return path.join(os.homedir(), ".spocket");
}
function isSpocketWorkspace(workspaceFile) {
    if (!workspaceFile) {
        return undefined;
    }
    const filePath = workspaceFile.fsPath;
    const spocketDir = getSpocketDir();
    if (!filePath.startsWith(spocketDir)) {
        return undefined;
    }
    // The pocket dir is the parent of the workspace file
    return path.dirname(filePath);
}
function getBinaryPath() {
    const config = vscode.workspace.getConfiguration("spocket");
    return config.get("binaryPath") || "spocket";
}
function runSync(pocketDir) {
    return new Promise((resolve) => {
        const binary = getBinaryPath();
        (0, child_process_1.execFile)(binary, ["sync", "--pocket", pocketDir], (error, stdout) => {
            if (error) {
                resolve({
                    status: "error",
                    message: error.message,
                });
                return;
            }
            try {
                const result = JSON.parse(stdout.trim());
                resolve(result);
            }
            catch {
                resolve({
                    status: "error",
                    message: `Failed to parse sync output: ${stdout}`,
                });
            }
        });
    });
}
function updateStatusBar(text, tooltip) {
    if (statusBarItem) {
        statusBarItem.text = text;
        if (tooltip) {
            statusBarItem.tooltip = tooltip;
        }
    }
}
async function handleFolderChange(pocketDir) {
    if (syncInProgress) {
        return;
    }
    syncInProgress = true;
    updateStatusBar("$(sync~spin) Spocket syncing...");
    try {
        const result = await runSync(pocketDir);
        switch (result.status) {
            case "unchanged":
                updateStatusBar(`$(check) Spocket`, `Hash: ${result.hash}\nBirth: ${result.birth_hash}`);
                break;
            case "synced": {
                updateStatusBar(`$(check) Spocket`, `Hash: ${result.new_hash}\nBirth: ${result.birth_hash}`);
                // Count added/removed for the notification
                const added = (result.paths?.length ?? 0) -
                    ((result.paths?.length ?? 0) -
                        ((result.new_hash !== result.old_hash ? 1 : 0) > 0 ? 1 : 0));
                vscode.window.setStatusBarMessage(`Spocket: manifest synced (${result.old_hash?.slice(0, 8)} → ${result.new_hash?.slice(0, 8)})`, 5000);
                break;
            }
            case "error":
                updateStatusBar("$(warning) Spocket", `Error: ${result.message}`);
                vscode.window.showWarningMessage(`Spocket sync failed: ${result.message}`);
                break;
        }
    }
    catch (err) {
        updateStatusBar("$(error) Spocket", "Sync failed");
    }
    finally {
        syncInProgress = false;
    }
}
function activate(context) {
    const pocketDir = isSpocketWorkspace(vscode.workspace.workspaceFile);
    if (!pocketDir) {
        // Not a spocket workspace — silent no-op
        return;
    }
    // Create status bar item
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    statusBarItem.text = "$(check) Spocket";
    statusBarItem.tooltip = `Pocket: ${path.basename(pocketDir)}`;
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);
    // Register folder change listener
    const disposable = vscode.workspace.onDidChangeWorkspaceFolders(() => {
        handleFolderChange(pocketDir);
    });
    context.subscriptions.push(disposable);
    // Initial sync to ensure manifest is up to date
    handleFolderChange(pocketDir);
}
function deactivate() {
    statusBarItem?.dispose();
    statusBarItem = undefined;
}
//# sourceMappingURL=extension.js.map