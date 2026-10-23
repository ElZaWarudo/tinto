import { useLayoutEffect, useRef, type TextareaHTMLAttributes } from "react";

/** A textarea as tall as its text, so a long text is read and edited whole
 *  instead of scrolled inside a box of fixed rows. `rows` stays its minimum. */
export function GrowingTextarea(props: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  const ref = useRef<HTMLTextAreaElement>(null);
  useLayoutEffect(() => {
    const area = ref.current;
    if (!area) return;
    area.style.height = "auto";
    // scrollHeight leaves out the borders; add them back so nothing scrolls.
    area.style.height = `${area.scrollHeight + area.offsetHeight - area.clientHeight}px`;
  }, [props.value]);
  return <textarea ref={ref} {...props} />;
}
