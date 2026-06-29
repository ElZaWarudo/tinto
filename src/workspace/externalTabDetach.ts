let activeCleanup: (() => void) | null = null;

type DragNativeEvent = DragEvent | PointerEvent;

export function armExternalTabDetach(
  nativeEvent: DragNativeEvent,
  detach: () => void | Promise<unknown>,
): () => void {
  activeCleanup?.();

  const sourceWindow = nativeEvent.view ?? window;
  const sourceDocument = sourceWindow.document;
  const rootElement = sourceDocument.documentElement;
  const start = eventPoint(nativeEvent);
  let cleaned = false;
  let releasedOutside = false;

  tryCapturePointer(nativeEvent);

  const cleanup = () => {
    if (cleaned) return;
    cleaned = true;
    sourceWindow.removeEventListener("pointermove", onPointerMove, true);
    sourceWindow.removeEventListener("pointerup", onRelease, true);
    sourceWindow.removeEventListener("mouseup", onRelease, true);
    sourceWindow.removeEventListener("dragover", onDragMove, true);
    sourceWindow.removeEventListener("dragend", onDragEnd, true);
    sourceWindow.removeEventListener("drop", onCancel, true);
    sourceWindow.removeEventListener("mouseout", onLeave, true);
    sourceWindow.removeEventListener("pointerout", onLeave, true);
    sourceDocument.removeEventListener("mouseleave", onLeave, true);
    sourceDocument.removeEventListener("pointerleave", onLeave, true);
    rootElement.removeEventListener("mouseleave", onLeave, true);
    rootElement.removeEventListener("pointerleave", onLeave, true);
    if (activeCleanup === cleanup) {
      activeCleanup = null;
    }
  };

  const runDetach = () => {
    if (cleaned) return;
    cleanup();
    void detach();
  };

  const updateOutsideState = (event: MouseEvent | PointerEvent | DragEvent, force = false) => {
    const point = eventPoint(event);
    releasedOutside =
      hasMovedEnough(start, point) && (force || isOutsideViewport(sourceWindow, point));
  };

  const detachIfReleasedOutside = (event: MouseEvent | PointerEvent | DragEvent) => {
    updateOutsideState(event);
    if (!releasedOutside) return;
    event.preventDefault();
    runDetach();
  };

  function onPointerMove(event: PointerEvent) {
    updateOutsideState(event);
  }

  function onDragMove(event: DragEvent) {
    updateOutsideState(event);
  }

  function onLeave(event: MouseEvent | PointerEvent) {
    if (event.relatedTarget) return;
    updateOutsideState(event, true);
  }

  function onDragEnd(event: DragEvent) {
    detachIfReleasedOutside(event);
    cleanup();
  }

  function onRelease(event: MouseEvent | PointerEvent) {
    detachIfReleasedOutside(event);
    cleanup();
  }

  function onCancel() {
    cleanup();
  }

  sourceWindow.addEventListener("pointermove", onPointerMove, true);
  sourceWindow.addEventListener("pointerup", onRelease, true);
  sourceWindow.addEventListener("mouseup", onRelease, true);
  sourceWindow.addEventListener("dragover", onDragMove, true);
  sourceWindow.addEventListener("dragend", onDragEnd, true);
  sourceWindow.addEventListener("drop", onCancel, true);
  sourceWindow.addEventListener("mouseout", onLeave, true);
  sourceWindow.addEventListener("pointerout", onLeave, true);
  sourceDocument.addEventListener("mouseleave", onLeave, true);
  sourceDocument.addEventListener("pointerleave", onLeave, true);
  rootElement.addEventListener("mouseleave", onLeave, true);
  rootElement.addEventListener("pointerleave", onLeave, true);
  activeCleanup = cleanup;

  return cleanup;
}

function tryCapturePointer(event: DragNativeEvent) {
  if (
    typeof PointerEvent === "undefined" ||
    !(event instanceof PointerEvent) ||
    !(event.target instanceof Element)
  ) {
    return;
  }
  try {
    event.target.setPointerCapture(event.pointerId);
  } catch {
    /* Some drag sources cannot capture the pointer. The release listeners still apply. */
  }
}

function eventPoint(event: MouseEvent | PointerEvent | DragEvent) {
  return {
    x: event.clientX,
    y: event.clientY,
  };
}

function hasMovedEnough(start: { x: number; y: number }, point: { x: number; y: number }) {
  return Math.hypot(point.x - start.x, point.y - start.y) > 24;
}

function isOutsideViewport(sourceWindow: Window, point: { x: number; y: number }) {
  return (
    point.x < 0 ||
    point.y < 0 ||
    point.x > sourceWindow.innerWidth ||
    point.y > sourceWindow.innerHeight
  );
}
