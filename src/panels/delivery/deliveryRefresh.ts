import { createContext } from "react";

/** What the Delivery panel does after any action: read the overview again. */
export const DeliveryRefreshContext = createContext<() => Promise<unknown>>(() =>
  Promise.resolve(),
);
