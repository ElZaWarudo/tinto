import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { MobileLinkStatus } from "../../delivery/mobileLink";

const off: MobileLinkStatus = { relay: "wss://relay.test", devices: [], pairing: null };
const asked: MobileLinkStatus = {
  ...off,
  pairing: {
    link: "tinto-link://relay.test/ROOM#KEY",
    qr_svg: "<svg></svg>",
    expires_at_ms: 0,
    relay: true,
    error: null,
    device: "Pixel de Ana",
  },
};
const paired: MobileLinkStatus = {
  ...off,
  devices: [
    {
      id: "ROOM",
      name: "Pixel de Ana",
      paired_at_ms: 0,
      relay: true,
      online: true,
      error: null,
    },
  ],
};

const client = vi.hoisted(() => ({
  getMobileLinkStatus: vi.fn(),
  setMobileLinkRelay: vi.fn(),
  startMobilePairing: vi.fn(),
  confirmMobilePairing: vi.fn(),
  cancelMobilePairing: vi.fn(),
  revokeMobileDevice: vi.fn(),
  onMobileLinkChanged: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("../../delivery/mobileLink", () => client);

import { MobileLinkSettings } from "./MobileLinkSettings";

describe("MobileLinkSettings", () => {
  it("pairs a phone the user confirms by name, and revokes it", async () => {
    const user = userEvent.setup();
    client.getMobileLinkStatus.mockResolvedValue(off);
    client.startMobilePairing.mockResolvedValue(asked);
    client.confirmMobilePairing.mockResolvedValue(paired);
    client.revokeMobileDevice.mockResolvedValue(off);
    render(<MobileLinkSettings />);

    await user.click(await screen.findByRole("button", { name: "Vincular móvil" }));
    expect(screen.getByAltText("Código QR para vincular el móvil")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("Pixel de Ana quiere vincularse");

    await user.click(screen.getByRole("button", { name: "Confirmar" }));
    expect(client.confirmMobilePairing).toHaveBeenCalledTimes(1);
    const list = await screen.findByRole("list", { name: "Móviles vinculados" });
    expect(list).toHaveTextContent("Pixel de Ana · En línea");

    await user.click(screen.getByRole("button", { name: "Revocar" }));
    expect(client.revokeMobileDevice).toHaveBeenCalledWith("ROOM");
  });
});
