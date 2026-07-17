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
  /** Avisos no fatales de limpieza o recuperación que deben mostrarse al usuario. */
  warnings?: string[];
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
  consoleLabel: string,
  userMessage: string,
): Promise<FileOpReport> {
  try {
    const result = await promise;
    return {
      copied: result.copied,
      conflicts: result.conflicts,
      warnings: result.warnings ?? [],
    };
  } catch (error: unknown) {
    console.error(`tinto: ${consoleLabel}`, error);
    return { copied: [], conflicts: [], fatalError: userMessage };
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
      "copy to repo failed",
      "No se copiaron los archivos. Comprueba que el origen y el destino sigan disponibles y vuelve a intentarlo.",
    );
  }
  // Si el caller pidió move pero es OS->repo, hacemos copy siempre; no
  // removemos archivos del filesystem del usuario sin consentimiento explícito.
  return runWithConflictSurface(
    copyToRepo(params.repo, params.destDir, params.sources, params.overwrite),
    "copy to repo failed",
    "No se copiaron los archivos. Comprueba que el origen y el destino sigan disponibles y vuelve a intentarlo.",
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
  return runWithConflictSurface(
    promise,
    params.strategy === "move" ? "move within repo failed" : "copy within repo failed",
    params.strategy === "move"
      ? "No se movieron los archivos. Comprueba que sigan disponibles y vuelve a intentarlo."
      : "No se copiaron los archivos. Comprueba que el origen y el destino sigan disponibles y vuelve a intentarlo.",
  );
}

/** Exporta archivos del repo hacia un directorio del OS. */
export async function sendToOs(params: {
  repo: string;
  sources: string[]; // relativas al repo
  destDir: string; // absoluto del OS
}): Promise<FileOpReport> {
  try {
    const outcome = await exportFromRepo(params.repo, params.sources, params.destDir);
    return { copied: [], conflicts: [], warnings: outcome?.warnings ?? [] };
  } catch (error: unknown) {
    console.error("tinto: export from repo failed", error);
    return {
      copied: [],
      conflicts: [],
      fatalError:
        "No se exportaron los archivos. Comprueba la carpeta de destino y vuelve a intentarlo.",
    };
  }
}

/** Elimina archivos o carpetas dentro del repo. */
export async function deleteWithinRepo(params: {
  repo: string;
  sources: string[]; // relativas al repo
  userConsent: boolean;
}): Promise<DeleteOpReport> {
  try {
    const deleteResult = await deleteFromRepo(params.repo, params.sources, params.userConsent);
    return {
      copied: [],
      conflicts: [],
      deleteResult,
      warnings: deleteResult.warnings ?? [],
    };
  } catch (error: unknown) {
    console.error("tinto: delete from repo failed", error);
    return {
      copied: [],
      conflicts: [],
      fatalError: "No se eliminó el elemento. Comprueba que siga disponible y vuelve a intentarlo.",
    };
  }
}

export async function restoreDeletedWithinRepo(params: {
  repo: string;
  token: string;
}): Promise<FileOpReport> {
  try {
    const outcome = await restoreDeletedFromRepo(params.repo, params.token);
    return { copied: [], conflicts: [], warnings: outcome?.warnings ?? [] };
  } catch (error: unknown) {
    console.error("tinto: restore deleted item failed", error);
    return {
      copied: [],
      conflicts: [],
      fatalError:
        "No se restauró el elemento. Puedes volver a intentarlo desde Tinto mientras siga abierto.",
    };
  }
}

export async function redoDeletedWithinRepo(params: {
  repo: string;
  token: string;
}): Promise<FileOpReport> {
  try {
    const outcome = await redoDeletedFromRepo(params.repo, params.token);
    return { copied: [], conflicts: [], warnings: outcome?.warnings ?? [] };
  } catch (error: unknown) {
    console.error("tinto: redo deleted item failed", error);
    return {
      copied: [],
      conflicts: [],
      fatalError:
        "No se repitió la eliminación. Comprueba que el elemento siga disponible y vuelve a intentarlo.",
    };
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
