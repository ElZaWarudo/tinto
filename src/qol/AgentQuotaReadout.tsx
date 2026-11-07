// One line with how much of each agent account's usage limit is spent, as its
// Delivery jobs or Claude Code consoles last reported it. Tinto never asks the
// provider: it only shows what the agents said.

import { useEffect, useState } from "react";
import { getAgentQuotas, onDeliveryChanged } from "../delivery/client";
import type { DeliveryQuota } from "../delivery/types";
import { quotaSummary } from "./quota";

export function AgentQuotaReadout({ nowMs, className }: { nowMs: number; className?: string }) {
  const [quotas, setQuotas] = useState<DeliveryQuota[]>([]);

  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | null = null;
    let pending: ReturnType<typeof setTimeout> | null = null;
    const load = async () => {
      try {
        const next = await getAgentQuotas();
        if (active) setQuotas(next ?? []);
      } catch {
        // Without Delivery's store there is nothing to show.
      }
    };
    void load();
    void onDeliveryChanged(() => {
      if (pending) return;
      // At most once a second: running agents change Delivery all the time.
      pending = setTimeout(() => {
        pending = null;
        void load();
      }, 1000);
    })
      .then((dispose) => {
        if (active) unlisten = dispose;
        else dispose();
      })
      .catch(() => {});
    return () => {
      active = false;
      unlisten?.();
      if (pending) clearTimeout(pending);
    };
  }, []);

  const summary = quotaSummary(quotas, nowMs);
  if (!summary) return null;
  return (
    <small className={className} data-testid="agent-quota" title="Uso del límite de cada cuenta">
      Límite de uso: {summary}
    </small>
  );
}
