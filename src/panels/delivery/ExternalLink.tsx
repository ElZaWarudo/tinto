// A link to a page outside Tinto (a Jira issue, a PR). The app's webview does
// not follow links that would open a new window, so the click asks the system
// to open it in the user's browser.

import type { ReactNode } from "react";
import { openDeliveryUrl } from "../../delivery/client";

export function ExternalLink({
  href,
  className,
  label,
  children,
}: {
  href: string;
  className?: string;
  label?: string;
  children: ReactNode;
}) {
  return (
    <a
      href={href}
      className={className}
      aria-label={label}
      rel="noreferrer"
      onClick={(event) => {
        event.preventDefault();
        void openDeliveryUrl(href).catch(() => {});
      }}
    >
      {children}
    </a>
  );
}
