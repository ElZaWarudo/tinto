import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { OverwriteConfirmModal } from "./OverwriteConfirmModal";

const report = {
  copied: [],
  conflicts: [
    { dest_rel: "src/existing.ts", kind: "file_exists" as const },
    { dest_rel: "assets", kind: "dir_exists" as const },
  ],
};

describe("OverwriteConfirmModal", () => {
  it("starts on the safe action and Enter on Cancelar never confirms", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<OverwriteConfirmModal report={report} onConfirm={onConfirm} onCancel={onCancel} />);

    expect(screen.getByTestId("overwrite-confirm-cancel")).toHaveFocus();
    await user.keyboard("{Enter}");

    expect(onCancel).toHaveBeenCalledOnce();
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("only confirms through the explicit overwrite action", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<OverwriteConfirmModal report={report} onConfirm={onConfirm} onCancel={onCancel} />);

    const confirm = screen.getByTestId("overwrite-confirm-ok");
    confirm.focus();
    await user.keyboard("{Enter}");

    expect(onConfirm).toHaveBeenCalledOnce();
    expect(onCancel).not.toHaveBeenCalled();
  });

  it("keeps keyboard focus inside and Escape cancels", async () => {
    const user = userEvent.setup();
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(<OverwriteConfirmModal report={report} onConfirm={onConfirm} onCancel={onCancel} />);

    const cancel = screen.getByTestId("overwrite-confirm-cancel");
    const confirm = screen.getByTestId("overwrite-confirm-ok");
    await user.tab();
    expect(confirm).toHaveFocus();
    await user.tab();
    expect(cancel).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(onCancel).toHaveBeenCalledOnce();
    expect(onConfirm).not.toHaveBeenCalled();
  });
});
