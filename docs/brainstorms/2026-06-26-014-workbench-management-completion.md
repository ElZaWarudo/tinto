---
title: Workbench selection and management completion
status: reviewed
date: 2026-06-26
roadmap_item: RDM-014
production_posture: prototype
---

# Workbench selection and management completion

## Request

Complete the workbench UX around creation, inspection, selection, rename, and deletion using the existing backend workbench commands. The user also requested retrofitting the already-landed commits into the Compound Master flow and handling follow-up changes with amend.

## Requirements

- R1: The app exposes a top-level Workbench menu even when there is one or zero workbenches.
- R2: The menu lists configured workbenches sorted by most-recently-used order, marks the active workbench, and switches via `set_active_workbench` plus snapshot reload.
- R3: Workbench creation trims the requested name, creates the backend workbench, activates it, reloads the active snapshot, and records it in MRU.
- R4: A manage-workbenches modal lists every configured workbench with its repo count and repo list.
- R5: The modal expands the active workbench by default and allows other workbenches to be expanded for inspection.
- R6: Rename is inline, trims input, no-ops on empty/unchanged values, calls the backend rename command, reloads config/snapshot, and moves the MRU entry from old name to new name.
- R7: Delete asks for destructive confirmation, deletes only the workbench config entry, does not delete repos on disk, removes the deleted name from MRU, and promotes the first remaining workbench only when the deleted workbench was active.
- R8: The UI tolerates partial config objects where `workbenches` is missing during first-run/recovery races.
- R9: MRU persistence is best-effort browser UI state only. The authoritative workbench set remains the backend `WorkbenchConfig`.
- R10: Tests cover MRU ordering, menu switching, modal open, create, activate, rename, delete, partial config tolerance, and operations wrapper behavior.

## Non-Goals

- No backend schema change or new Tauri command.
- No repo alias editing inside the manage modal.
- No per-workbench layout persistence.
- No physical repo deletion.
- No Jira/PR mutation from this Compound Master work phase.

## Assumptions

- Existing backend validation remains authoritative for duplicate, unknown, or invalid workbench names.
- MRU ordering by workbench name is acceptable because workbench names are unique in the backend config.
- When deleting the active workbench, choosing the first remaining backend order is acceptable until a richer selection policy is needed.
- The app is still prototype posture; focused component tests plus type/build gates are sufficient before amend.

## Acceptance Examples

- AE1: With MRU `["Side", "Work"]` and active `Work`, opening Workbench shows `Side` before `Work`, and `Work` is marked active.
- AE2: Creating `"  Sandbox  "` calls create/activate with `Sandbox`, reloads, clears the modal input, and marks `Sandbox` recent.
- AE3: Renaming `Work` to `Job` calls `rename_workbench("Work", "Job")`, reloads, removes `Work` from MRU, and records `Job`.
- AE4: Deleting active `Side` from `[Work, Side, Client X]` calls delete, promotes `Work`, resets the bus, reloads, and removes `Side` from MRU.
- AE5: Rendering the manage modal with `{ version: 1, active: null }` does not throw and still exposes the create form.

## Planning Input

Proceed with one plan and one work package for RDM-014. Keep review-unit granularity at one cohesive frontend capability because the menu, MRU helper, modal, and operations wrappers are all needed for the user-visible flow and have focused tests.
