// Shared workspace interactions for the F4 daily-driver specs. Complements
// cabinet-helpers.ts (whose primitives — uniq, useTallViewport, fileRow,
// openRowMenu, uploadFile, gotoUntil — are context-agnostic and imported
// directly) with workspace-scoped path building and the create-workspace
// flow. createWorkspace is duplicated from workspace-lifecycle.spec.ts's
// local helper rather than imported: spec files aren't a shared surface,
// only helper modules are.
import { expect, type Locator, type Page } from "@playwright/test";

/** Splat URL for a workspace path (no segments → the workspace root). */
export function workspacePath(rkey: string, ...segments: readonly string[]): string {
  return segments.length === 0
    ? `/cabinet/workspace/${rkey}`
    : `/cabinet/workspace/${rkey}/${segments.join("/")}`;
}

export function workspaceSettingsPath(rkey: string): string {
  return `/cabinet/workspace-settings/${rkey}`;
}

/** Create a workspace from /cabinet/files and wait for the SSE-echoed
 *  sidebar entry — there is no optimistic insert, so visibility means the
 *  indexer can already answer for it (immediately actionable). */
export async function createWorkspace(page: Page, name: string): Promise<Locator> {
  await page.goto("/cabinet/files");
  await page.getByRole("button", { name: "Create workspace" }).click();
  const dialog = page.getByRole("dialog", { name: "Create workspace" });
  await dialog.getByLabel("Workspace name").fill(name);
  await dialog.getByRole("button", { name: "Create", exact: true }).click();
  const link = page.getByRole("link", { name });
  await expect(link).toBeVisible({ timeout: 30_000 });
  return link;
}

/** rkey from the current /cabinet/workspace(-settings)/<rkey>[/...] URL. */
export function currentWorkspaceRkey(page: Page): string {
  const match = /\/cabinet\/workspace(?:-settings)?\/([^/]+)/.exec(new URL(page.url()).pathname);
  const rkey = match?.[1];
  if (!rkey) throw new Error(`could not derive workspace rkey from ${page.url()}`);
  return rkey;
}

/** A minimal valid 1x1 transparent PNG, for icon-upload tests — small enough
 *  to inline, real enough for the browser's <img>/canvas decode pipeline to
 *  accept it (the settings page reads it back through FileReader + Image). */
export const TINY_PNG_BASE64 =
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
