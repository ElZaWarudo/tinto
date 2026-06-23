let windowsHostOverride: boolean | null = null;

export function isWindowsHost(): boolean {
  if (windowsHostOverride !== null) return windowsHostOverride;
  if (typeof navigator === "undefined") return false;
  const nav = navigator as Navigator & { userAgentData?: { platform?: string } };
  const platform = nav.userAgentData?.platform ?? navigator.platform ?? "";
  return /\bwin/i.test(platform);
}

export function setWindowsHostOverrideForTests(value: boolean | null): void {
  windowsHostOverride = value;
}
