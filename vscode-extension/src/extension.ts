import * as vscode from "vscode";
import { execFile } from "child_process";
import * as path from "path";
import * as os from "os";

let statusBarItem: vscode.StatusBarItem | undefined;
let syncInProgress = false;

interface SyncResult {
  status: "unchanged" | "synced" | "error";
  hash?: string;
  old_hash?: string;
  new_hash?: string;
  birth_hash?: string;
  paths?: string[];
  message?: string;
}

function getSpocketDir(): string {
  return path.join(os.homedir(), ".spocket");
}

function isSpocketWorkspace(
  workspaceFile: vscode.Uri | undefined
): string | undefined {
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

function getBinaryPath(): string {
  const config = vscode.workspace.getConfiguration("spocket");
  return config.get<string>("binaryPath") || "spocket";
}

function runSync(pocketDir: string): Promise<SyncResult> {
  return new Promise((resolve) => {
    const binary = getBinaryPath();

    execFile(binary, ["sync", "--pocket", pocketDir], (error, stdout) => {
      if (error) {
        resolve({
          status: "error",
          message: error.message,
        });
        return;
      }

      try {
        const result: SyncResult = JSON.parse(stdout.trim());
        resolve(result);
      } catch {
        resolve({
          status: "error",
          message: `Failed to parse sync output: ${stdout}`,
        });
      }
    });
  });
}

function updateStatusBar(text: string, tooltip?: string): void {
  if (statusBarItem) {
    statusBarItem.text = text;
    if (tooltip) {
      statusBarItem.tooltip = tooltip;
    }
  }
}

async function handleFolderChange(pocketDir: string): Promise<void> {
  if (syncInProgress) {
    return;
  }

  syncInProgress = true;
  updateStatusBar("$(sync~spin) Spocket syncing...");

  try {
    const result = await runSync(pocketDir);

    switch (result.status) {
      case "unchanged":
        updateStatusBar(
          `$(check) Spocket`,
          `Hash: ${result.hash}\nBirth: ${result.birth_hash}`
        );
        break;

      case "synced": {
        updateStatusBar(
          `$(check) Spocket`,
          `Hash: ${result.new_hash}\nBirth: ${result.birth_hash}`
        );

        // Count added/removed for the notification
        const added =
          (result.paths?.length ?? 0) -
          ((result.paths?.length ?? 0) -
            ((result.new_hash !== result.old_hash ? 1 : 0) > 0 ? 1 : 0));

        vscode.window.setStatusBarMessage(
          `Spocket: manifest synced (${result.old_hash?.slice(0, 8)} → ${result.new_hash?.slice(0, 8)})`,
          5000
        );
        break;
      }

      case "error":
        updateStatusBar("$(warning) Spocket", `Error: ${result.message}`);
        vscode.window.showWarningMessage(
          `Spocket sync failed: ${result.message}`
        );
        break;
    }
  } catch (err) {
    updateStatusBar("$(error) Spocket", "Sync failed");
  } finally {
    syncInProgress = false;
  }
}

export function activate(context: vscode.ExtensionContext): void {
  const pocketDir = isSpocketWorkspace(vscode.workspace.workspaceFile);

  if (!pocketDir) {
    // Not a spocket workspace — silent no-op
    return;
  }

  // Create status bar item
  statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Right,
    100
  );
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

export function deactivate(): void {
  statusBarItem?.dispose();
  statusBarItem = undefined;
}
