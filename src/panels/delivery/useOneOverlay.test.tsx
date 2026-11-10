import { describe, it, expect, vi } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import { useRef } from "react";
import { useOneOverlay } from "./useOneOverlay";

function Float({ onClose }: { onClose: () => void }) {
  const surface = useRef<HTMLDivElement>(null);
  useOneOverlay(surface, null, onClose);
  return <div ref={surface}>float</div>;
}

describe("useOneOverlay", () => {
  it("stays open while a confirm asked from it is answered, and closes on a press outside", () => {
    const onClose = vi.fn();
    render(<Float onClose={onClose} />);
    // The confirm renders on the page's body, outside the float.
    const confirm = document.createElement("div");
    confirm.className = "file-op-modal-overlay";
    confirm.innerHTML = "<button>Cancelar</button>";
    document.body.appendChild(confirm);
    fireEvent.mouseDown(confirm.querySelector("button")!);
    expect(onClose).not.toHaveBeenCalled();
    confirm.remove();
    fireEvent.mouseDown(document.body);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
