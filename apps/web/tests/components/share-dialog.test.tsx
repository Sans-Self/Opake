// @vitest-environment happy-dom
import { createRef } from "react";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  resolveRecipient: vi.fn(),
  useFileManager: vi.fn(),
  toastError: vi.fn(),
  toastSuccess: vi.fn(),
  RecipientNotReadyError: class RecipientNotReadyError extends Error {},
}));

vi.mock("@opake/react", () => ({ useFileManager: mocks.useFileManager }));
vi.mock("@/stores/auth", () => ({
  useAuthStore: (selector: (state: unknown) => unknown) =>
    selector({ session: { status: "active", did: "did:plc:owner" } }),
}));
vi.mock("@/stores/toast", () => ({
  toastError: mocks.toastError,
  toastSuccess: mocks.toastSuccess,
}));
vi.mock("@/lib/sharing", () => ({
  RecipientNotReadyError: mocks.RecipientNotReadyError,
  resolveRecipient: mocks.resolveRecipient,
  recipientVerificationLabel: vi.fn(),
  toastWriteVerificationNotice: vi.fn(),
}));

import { ShareDialog, type ShareDialogHandle } from "@/components/cabinet/ShareDialog";

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  mocks.resolveRecipient.mockReset();
  mocks.useFileManager.mockReset();
  mocks.toastError.mockReset();
  mocks.toastSuccess.mockReset();
});

async function showNotReadyDialog(challenge: { did: string; dispose: ReturnType<typeof vi.fn> }) {
  const fileManager = {
    preparePendingShareRecipient: vi.fn(async () => challenge),
    createPendingShare: vi.fn(async () => "at://did:plc:owner/at.opake.pendingShare/one"),
  };
  mocks.useFileManager.mockReturnValue({ fileManager });
  mocks.resolveRecipient.mockRejectedValue(new mocks.RecipientNotReadyError("not ready"));

  const ref = createRef<ShareDialogHandle>();
  render(<ShareDialog ref={ref} />);
  act(() => ref.current?.show("at://did:plc:owner/at.opake.document/report", "Report"));
  fireEvent.change(screen.getByLabelText("Recipient handle"), {
    target: { value: "alice.test" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Share" }));
  await act(async () => {});
  expect(screen.getByRole("button", { name: "Queue share" })).toBeTruthy();
  return fileManager;
}

describe("queued-share consent", () => {
  it("does not queue when the owner declines first-publication consent", async () => {
    const challenge = { did: "did:plc:alice", dispose: vi.fn() };
    const fileManager = await showNotReadyDialog(challenge);
    vi.stubGlobal("confirm", vi.fn(() => false));

    fireEvent.click(screen.getByRole("button", { name: "Queue share" }));

    expect(globalThis.confirm).toHaveBeenCalledWith(
      "alice.test (did:plc:alice) has not published encryption keys. Their first keys may be unverified, so whoever runs their PDS could substitute them and read this document. Queue one automatic share to those first keys?",
    );
    expect(fileManager.createPendingShare).not.toHaveBeenCalled();
    expect(challenge.dispose).toHaveBeenCalledOnce();
  });

  it("queues with the exact opaque resolution shown in the warning", async () => {
    const challenge = { did: "did:plc:alice", dispose: vi.fn() };
    const fileManager = await showNotReadyDialog(challenge);
    vi.stubGlobal("confirm", vi.fn(() => true));

    fireEvent.click(screen.getByRole("button", { name: "Queue share" }));
    await act(async () => {});

    expect(fileManager.preparePendingShareRecipient).toHaveBeenCalledTimes(1);
    expect(fileManager.preparePendingShareRecipient).toHaveBeenCalledWith(
      "at://did:plc:owner/at.opake.document/report",
      "alice.test",
    );
    expect(fileManager.createPendingShare).toHaveBeenCalledWith(
      "at://did:plc:owner/at.opake.document/report",
      challenge,
      true,
      "read",
      null,
    );
  });

  it("keeps the warning DID readable while the queued write consumes its opaque handle", async () => {
    let consumed = false;
    const challenge = {
      get did() {
        if (consumed) throw new Error("PendingShareRecipient has already been consumed");
        return "did:plc:alice";
      },
      dispose: vi.fn(),
      consume: () => {
        consumed = true;
      },
    };
    const fileManager = await showNotReadyDialog(challenge as never);
    fileManager.createPendingShare.mockImplementation(async () => {
      challenge.consume();
      await new Promise((resolve) => setTimeout(resolve, 0));
      return "at://did:plc:owner/at.opake.pendingShare/one";
    });
    vi.stubGlobal("confirm", vi.fn(() => true));

    fireEvent.click(screen.getByRole("button", { name: "Queue share" }));
    expect(screen.getByText(/did:plc:alice/)).toBeTruthy();
    await act(async () => {});
    expect(fileManager.createPendingShare).toHaveBeenCalledOnce();
    expect(consumed).toBe(true);
  });
});
