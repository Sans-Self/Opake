// Real browser helper for tests that need a verified actor. The owner code is
// read only after the PDS signer has issued it, then submitted through the
// production verification holder; no test code calls the signer directly.
import type { Browser, BrowserContext, Page } from "@playwright/test";
import { expect, ACTORS, authFile, installBlockade, type Actor } from "./fixtures";
import { awaitPlcOwnerConfirmation, plcDidDocument } from "../helpers/devenv";
import { repositoryRecord } from "./pds-admin";
import { actorNamespace } from "./namespace";

const APP_ORIGIN = "http://127.0.0.1:5199";
const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

export interface VerifiedActor {
  readonly page: Page;
  readonly context: BrowserContext;
  readonly cleanup: () => Promise<void>;
}

function actor(name: string): Actor {
  const found = ACTORS.find((candidate) => candidate.name === name);
  if (!found) throw new Error(`fixture actor ${name} not found`);
  return found;
}

async function authorizePopup(page: Page, owner: Actor): Promise<void> {
  page.setDefaultTimeout(15_000);
  const assertNoEscape = await installBlockade(page);
  try {
    await page.waitForURL((url) => url.origin !== APP_ORIGIN, { timeout: 30_000 });

    const signIn = page.getByRole("button", { name: /^sign in$/i });
    const password = page.locator('input[type="password"]').first();
    const submittedCombinedForm = await (async () => {
      const signInVisible = await signIn
        .waitFor({ state: "visible", timeout: 2_000 })
        .then(() => true)
        .catch(() => false);
      if (!signInVisible) return false;
      await signIn.click();
      const identifier = page.locator('input[type="text"], input[type="email"]').first();
      await identifier.waitFor({ state: "visible", timeout: 8_000 });
      await identifier.fill(owner.handle);
      const passwordOnSignInForm = await password
        .waitFor({ state: "visible", timeout: 8_000 })
        .then(() => true)
        .catch(() => false);
      if (passwordOnSignInForm) {
        await password.fill(owner.password);
      }
      await page.locator('button[type="submit"]').first().click();
      return passwordOnSignInForm;
    })();
    if (
      !submittedCombinedForm &&
      (await password
        .waitFor({ state: "visible", timeout: 2_000 })
        .then(() => true)
        .catch(() => false))
    ) {
      await password.fill(owner.password);
      await page.locator('button[type="submit"]').first().click();
    }
    const authorize = page.getByRole("button", { name: /authorize|accept|allow/i }).first();
    if (
      await authorize
        .waitFor({ state: "visible", timeout: 8_000 })
        .then(() => true)
        .catch(() => false)
    ) {
      await authorize.click();
    }

    await page.waitForURL((url) => url.origin === APP_ORIGIN, { timeout: 30_000 });
  } finally {
    assertNoEscape();
  }
}

function decodeBase58(value: string): Uint8Array {
  const bytes = [0];
  for (const character of value) {
    const digit = BASE58.indexOf(character);
    if (digit < 0) throw new Error("invalid base58 PLC key");
    let carry = digit;
    for (let index = 0; index < bytes.length; index += 1) {
      carry += bytes[index]! * 58;
      bytes[index] = carry & 0xff;
      carry >>= 8;
    }
    while (carry > 0) {
      bytes.push(carry & 0xff);
      carry >>= 8;
    }
  }
  for (const character of value) {
    if (character !== "1") break;
    bytes.push(0);
  }
  return Uint8Array.from(bytes.reverse());
}

async function assertPublishedAnchor(owner: Actor): Promise<void> {
  const record = await repositoryRecord(owner, "at.opake.publicKey", "self");
  const signingKey = (record?.value as { signingKey?: { $bytes?: unknown } })?.signingKey?.$bytes;
  if (typeof signingKey !== "string") throw new Error("verified actor has no signing key record");
  const document = await plcDidDocument(owner.name);
  const methods = document.verificationMethod;
  if (!Array.isArray(methods)) throw new Error("PLC DID document omitted verificationMethod");
  const method = methods.find(
    (entry): entry is { id: string; publicKeyMultibase: string } =>
      !!entry &&
      typeof entry === "object" &&
      typeof (entry as { id?: unknown }).id === "string" &&
      (entry as { id: string }).id.endsWith("#opake") &&
      typeof (entry as { publicKeyMultibase?: unknown }).publicKeyMultibase === "string",
  );
  if (!method) throw new Error("PLC DID document did not publish #opake");
  const decoded = decodeBase58(method.publicKeyMultibase.slice(1));
  if (method.publicKeyMultibase[0] !== "z" || decoded[0] !== 0xed || decoded[1] !== 0x01) {
    throw new Error("PLC #opake method was not an Ed25519 multikey");
  }
  expect(decoded.slice(2)).toEqual(Uint8Array.from(Buffer.from(signingKey, "base64")));
}

async function submitIdentityOperation(
  page: Page,
  owner: Actor,
  label: "Set up verification" | "Remove verification",
): Promise<void> {
  const popupCreated = page.context().waitForEvent("page");
  const requestedAfter = Date.now();
  await page.getByRole("button", { name: label, exact: true }).click();
  await authorizePopup(await popupCreated, owner);
  await expect(page.getByLabel("Owner confirmation")).toBeVisible({ timeout: 30_000 });
  const confirmation = await awaitPlcOwnerConfirmation(owner.name, requestedAfter);
  await page.getByLabel("Owner confirmation").fill(confirmation.code);
  await page.getByRole("button", { name: "Confirm", exact: true }).click();
}

/**
 * Open one namespaced actor in its own authenticated browser context and
 * publish a signed #opake verification method if it is not already present.
 * `cleanup` removes only that method before closing the context.
 */
export async function ensureVerifiedActor(
  browser: Browser,
  actorName: string,
): Promise<VerifiedActor> {
  const owner = actor(actorName);
  if (actorNamespace() === "" && !owner.verified) {
    throw new Error(
      "verification may mutate only a manifest-verified actor in the default fixture namespace",
    );
  }
  const context = await browser.newContext({
    storageState: authFile(owner.name),
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  const assertNoEscape = await installBlockade(page);

  try {
    await page.goto("/cabinet/settings");
    const setup = page.getByRole("button", { name: "Set up verification", exact: true });
    const remove = page.getByRole("button", { name: "Remove verification", exact: true });
    await expect(setup.or(remove)).toBeVisible({ timeout: 30_000 });
    if (await setup.isVisible().catch(() => false)) {
      await submitIdentityOperation(page, owner, "Set up verification");
    }
    await expect(remove).toBeVisible({ timeout: 30_000 });
    await assertPublishedAnchor(owner);
    assertNoEscape();

    return {
      page,
      context,
      cleanup: async () => {
        try {
          await page.goto("/cabinet/settings");
          if (await remove.isVisible().catch(() => false)) {
            await submitIdentityOperation(page, owner, "Remove verification");
            await expect(setup).toBeVisible({ timeout: 30_000 });
          }
          assertNoEscape();
        } finally {
          await context.close();
        }
      },
    };
  } catch (error) {
    await context.close();
    throw error;
  }
}
