// File-operation orchestration for the explorer: wraps the backend copy/move
// commands with conflict handling. When the backend reports conflicts (a
// destination already exists), this surfaces them to the caller so the UI can
// ask the user to confirm the overwrite and retry with overwrite=true.

import {
  copyToRepo,
  copyWithinRepo,
  moveWithinRepo,
  exportFromRepo,
  deleteFromRepo,
  restoreDeletedFromRepo,
  redoDeletedFromRepo,
} from "../../bus/client";
import type { CopyResult, DeleteResult, FileConflict } from "../../bus/contract";

export type CopyStrategy = "copy" | "move";
export type RetryFn = () => Promise<void> | void;
export interface FileOpReport {
  copied: string[];
  conflicts: FileConflict[];
  /** Error fatal si el comando falló con algo que no son conflictos. */
  fatalError?: string;
}

export interface DeleteOpReport extends FileOpReport {
  deleteResult?: DeleteResult;
}

export class FatalFileOpError extends Error {}

/** Llama al comando backend y normaliza el resultado. */
async function runWithConflictSurface(
  promise: Promise<CopyResult>,
  fatalCategory: string,
): Promise<FileOpReport> {
  try {
    const result = await promise;
    return { copied: result.copied, conflicts: result.conflicts };
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    return { copied: [], conflicts: [], fatalError: `${fatalCategory}: ${message}` };
  }
}

/**
 * Copia o mueve archivos desde el OS hacia una carpeta del repo. Si el backend
 * reporta conflictos, retorna sin fatal error para que el frontend muestre la
 * confirmación; al confirmar, el caller reintenta con `overwrite=true`.
 */
export async function sendFromOs(params: {
  repo: string;
  destDir: string; // relativo al repo; "" = raíz
  sources: string[]; // paths absolutos del OS
  strategy: CopyStrategy;
  overwrite: boolean;
}): Promise<FileOpReport> {
  // OS -> repo siempre es copia (no queremos mover archivos del SO del usuario).
  if (params.strategy === "copy") {
    return runWithConflictSurface(
      copyToRepo(params.repo, params.destDir, params.sources, params.overwrite),
      "copy_to_repo",
    );
  }
  // Si el caller pidió move pero es OS->repo, hacemos copy siempre; no
  // removemos archivos del filesystem del usuario sin consentimiento explícito.
  return runWithConflictSurface(
    copyToRepo(params.repo, params.destDir, params.sources, params.overwrite),
    "copy_to_repo",
  );
}

/** Copia o mueve archivos dentro del mismo repo. */
export async function sendWithinRepo(params: {
  repo: string;
  sources: string[]; // relativas al repo
  destDir: string; // relativa al repo
  strategy: CopyStrategy;
  overwrite: boolean;
}): Promise<FileOpReport> {
  const promise =
    params.strategy === "move"
      ? moveWithinRepo(params.repo, params.sources, params.destDir, params.overwrite)
      : copyWithinRepo(params.repo, params.sources, params.destDir, params.overwrite);
  return runWithConflictSurface(promise, `${params.strategy}_within_repo`);
}

/** Exporta archivos del repo hacia un directorio del OS. */
export async function sendToOs(params: {
  repo: string;
  sources: string[]; // relativas al repo
  destDir: string; // absoluto del OS
}): Promise<FileOpReport> {
  try {
    await exportFromRepo(params.repo, params.sources, params.destDir);
    return { copied: [], conflicts: [] };
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    return { copied: [], conflicts: [], fatalError: `export_from_repo: ${message}` };
  }
}

/** Elimina archivos o carpetas dentro del repo. */
export async function deleteWithinRepo(params: {
  repo: string;
  sources: string[]; // relativas al repo
}): Promise<DeleteOpReport> {
  try {
    const deleteResult = await deleteFromRepo(params.repo, params.sources);
    return { copied: [], conflicts: [], deleteResult };
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    return { copied: [], conflicts: [], fatalError: `delete_from_repo: ${message}` };
  }
}

export async function restoreDeletedWithinRepo(params: {
  repo: string;
  token: string;
}): Promise<FileOpReport> {
  try {
    await restoreDeletedFromRepo(params.repo, params.token);
    return { copied: [], conflicts: [] };
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    return { copied: [], conflicts: [], fatalError: `restore_deleted_from_repo: ${message}` };
  }
}

export async function redoDeletedWithinRepo(params: {
  repo: string;
  token: string;
}): Promise<FileOpReport> {
  try {
    await redoDeletedFromRepo(params.repo, params.token);
    return { copied: [], conflicts: [] };
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    return { copied: [], conflicts: [], fatalError: `redo_deleted_from_repo: ${message}` };
  }
}

/** ¿Este reporte tiene conflictos que requieren confirmación del usuario? */
export function needsConfirmation(report: FileOpReport): boolean {
  return report.conflicts.some((c) => c.kind === "file_exists" || c.kind === "dir_exists");
}

/** Mensaje legible para un conflicto. */
export function conflictDescription(conflict: FileConflict): string {
  switch (conflict.kind) {
    case "file_exists":
      return `Ya existe el archivo ${conflict.dest_rel}`;
    case "dir_exists":
      return `Ya existe un directorio ${conflict.dest_rel}`;
    case "source_missing":
      return `No se encuentra ${conflict.dest_rel} (¿se movió?)`;
    case "overwrite":
      return `Sobreescrito ${conflict.dest_rel}`;
  }
}
