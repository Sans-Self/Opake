// @vitest-environment happy-dom

// Accessibility regressions for issue #17: cabinet row action-menu triggers
// and the RenameDialog input must carry explicit accessible names. These are
// contract tests — they assert what an assistive technology can perceive
// (roles + accessible names), not markup internals.

import { createRef } from "react";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { FileActionMenu } from "@/components/cabinet/FileActionMenu";
import { DropdownMenu } from "@/components/DropdownMenu";
import { RenameDialog, type RenameDialogHandle } from "@/components/cabinet/RenameDialog";
import type { FileItem } from "@/components/cabinet/types";
import { EyeIcon } from "@phosphor-icons/react";

afterEach(cleanup);

function fileItem(overrides: Partial<FileItem> = {}): FileItem {
  return {
    id: "1",
    uri: "at://did:example/app.opake.file/1",
    name: "Quarterly Report.pdf",
    kind: "file",
    encrypted: true,
    status: "private",
    modified: "2026-07-15",
    decrypted: true,
    tags: [],
    ...overrides,
  };
}

describe("FileActionMenu accessible name", () => {
  it("names the file row action trigger after the item", () => {
    render(<FileActionMenu item={fileItem({ name: "Quarterly Report.pdf" })} />);
    expect(screen.getByRole("button", { name: "Actions for Quarterly Report.pdf" })).toBeInstanceOf(
      HTMLButtonElement,
    );
  });

  it("names the folder row action trigger after the item", () => {
    render(<FileActionMenu item={fileItem({ kind: "folder", name: "Invoices" })} />);
    expect(screen.getByRole("button", { name: "Actions for Invoices" })).toBeInstanceOf(
      HTMLButtonElement,
    );
  });

  it("gives distinct accessible names to two rows so they are distinguishable", () => {
    render(
      <>
        <FileActionMenu item={fileItem({ id: "a", name: "Alpha.pdf" })} />
        <FileActionMenu item={fileItem({ id: "b", name: "Beta.pdf" })} />
      </>,
    );
    expect(screen.getByRole("button", { name: "Actions for Alpha.pdf" })).toBeInstanceOf(
      HTMLButtonElement,
    );
    expect(screen.getByRole("button", { name: "Actions for Beta.pdf" })).toBeInstanceOf(
      HTMLButtonElement,
    );
  });
});

describe("DropdownMenu triggerLabel", () => {
  it("exposes triggerLabel as the trigger's accessible name", () => {
    render(
      <DropdownMenu
        trigger={<EyeIcon size={16} />}
        triggerLabel="Actions for Report.pdf"
        items={[{ icon: EyeIcon, label: "Preview" }]}
      />,
    );
    expect(screen.getByRole("button", { name: "Actions for Report.pdf" })).toBeInstanceOf(
      HTMLButtonElement,
    );
  });
});

describe("RenameDialog label association", () => {
  it("associates the name input with its visible label", () => {
    const ref = createRef<RenameDialogHandle>();
    render(<RenameDialog ref={ref} onSave={() => {}} />);
    act(() => ref.current?.show("at://did:example/app.opake.directory/1", "Old name"));
    // getByLabelText resolves through the label association — it fails if the
    // input is reachable only by implicit wrapping in some engines.
    expect(screen.getByLabelText("Name")).toBe(screen.getByRole("textbox", { name: "Name" }));
  });
});
