import type { DeleteResult } from "../../bus/contract";
import { repoTreeStore } from "../../workspace/repoTreeStore";
import { redoDeletedWithinRepo, restoreDeletedWithinRepo, type FileOpReport } from "./fileOps";

interface DeleteHistoryEntry {
  repo: string;
  token: string;
}

class DeleteUndoManager {
  private undoStack: DeleteHistoryEntry[] = [];
  private redoStack: DeleteHistoryEntry[] = [];

  recordDelete(repo: string, result: DeleteResult): void {
    this.undoStack.push({ repo, token: result.token });
    this.redoStack = [];
  }

  async undo(): Promise<FileOpReport | null> {
    const op = this.undoStack.pop();
    if (!op) return null;
    const report = await restoreDeletedWithinRepo({ repo: op.repo, token: op.token });
    if (report.fatalError) {
      this.undoStack.push(op);
      return report;
    }
    this.redoStack.push(op);
    repoTreeStore.refresh(op.repo);
    return report;
  }

  async redo(): Promise<FileOpReport | null> {
    const op = this.redoStack.pop();
    if (!op) return null;
    const report = await redoDeletedWithinRepo({ repo: op.repo, token: op.token });
    if (report.fatalError) {
      this.redoStack.push(op);
      return report;
    }
    this.undoStack.push(op);
    repoTreeStore.refresh(op.repo);
    return report;
  }

  reset(): void {
    this.undoStack = [];
    this.redoStack = [];
  }
}

export const deleteUndoManager = new DeleteUndoManager();
