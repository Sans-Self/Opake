// Production verification-method proof. The browser drives the real web/WASM
// holder and OAuth popup. The only test fixture is the owner code: it is read
// from the disposable actor's local PDS SQLite token row, then submitted back
// through the normal UI so PDS still validates and consumes it.
import type { Page } from "@playwright/test";
import { blockadeTest as test, expect, ACTORS, authFile, installBlockade } from "../fixtures";
import {
  awaitPlcOwnerConfirmation,
  pdsOAuthScopeSummary,
  plcDidDocument,
  plcOperationLog,
} from "../../helpers/devenv";
import { repositoryCommit, repositoryRecord } from "../pds-admin";
import { actorNamespace } from "../namespace";
import { gotoCabinetRoot, uploadFile } from "../cabinet-helpers";

const APP_ORIGIN = "http://127.0.0.1:5199";
const PUBLIC_KEY_COLLECTION = "at.opake.publicKey";
const PUBLIC_KEY_RKEY = "self";

const actor = (name: string) => {
  const result = ACTORS.find((candidate) => candidate.name === name);
  if (!result) throw new Error(`fixture actor ${name} not found`);
  return result;
};

function currentHasOpake(log: readonly Record<string, unknown>[]): boolean {
  const methods = log.at(-1)?.verificationMethods;
  return !!methods && typeof methods === "object" && "opake" in methods;
}

function currentOperation(log: readonly Record<string, unknown>[]): Record<string, unknown> {
  const operation = log.at(-1);
  if (!operation) throw new Error("PLC operation log was empty");
  return operation;
}

const BASE58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

function decodeBase58(value: string): Uint8Array {
  const bytes = [0];
  for (const char of value) {
    const digit = BASE58.indexOf(char);
    if (digit < 0) throw new Error("invalid base58 character in PLC key");
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
  for (const char of value) {
    if (char !== "1") break;
    bytes.push(0);
  }
  return Uint8Array.from(bytes.reverse());
}

function plcPublicKey(value: unknown): Uint8Array {
  const multibase = typeof value === "string" ? value.replace(/^did:key:/, "") : value;
  if (typeof multibase !== "string" || !multibase.startsWith("z")) {
    throw new Error("PLC verification key was not base58btc multibase");
  }
  const decoded = decodeBase58(multibase.slice(1));
  if (decoded[0] !== 0xed || decoded[1] !== 0x01 || decoded.length !== 34) {
    throw new Error("PLC verification key was not an Ed25519 multicodec key");
  }
  return decoded.slice(2);
}

function expectedSigningKey(record: unknown): Uint8Array {
  const encoded = (record as { signingKey?: { $bytes?: unknown } }).signingKey?.$bytes;
  if (typeof encoded !== "string") throw new Error("signed public-key record omitted signingKey");
  return Uint8Array.from(Buffer.from(encoded, "base64"));
}

function opakeKeyFromDocument(document: Record<string, unknown>): Uint8Array | null {
  const methods = document.verificationMethod;
  if (!Array.isArray(methods)) throw new Error("PLC DID document omitted verificationMethod");
  const method = methods.find(
    (entry): entry is Record<string, unknown> =>
      !!entry &&
      typeof entry === "object" &&
      typeof entry.id === "string" &&
      entry.id.endsWith("#opake"),
  );
  return method ? plcPublicKey(method.publicKeyMultibase) : null;
}

async function authorizePopup(page: Page, owner: ReturnType<typeof actor>): Promise<string> {
  const assertNoEscape = await installBlockade(page);
  try {
    console.info("[verification-e2e] popup: waiting for authorization server");
    await page.waitForURL((url) => url.protocol !== "about:" && url.origin !== APP_ORIGIN, {
      timeout: 30_000,
    });

    // The auth setup snapshot normally carries a PDS browser session. Retaining
    // this branch makes a freshly-created namespace resilient to a PDS session
    // expiry while keeping the interaction on the production OAuth screen.
    const signIn = page.getByRole("button", { name: /^sign in$/i });
    const enteredIdentifier = await signIn
      .waitFor({ state: "visible", timeout: 2_000 })
      .then(() => true)
      .catch(() => false);
    if (enteredIdentifier) {
      console.info("[verification-e2e] popup: signing in");
      await signIn.click();
      const identifier = page.locator('input[type="text"], input[type="email"]').first();
      await identifier.waitFor({ state: "visible", timeout: 8_000 });
      await identifier.fill(owner.handle);
    }
    const password = page.locator('input[type="password"]').first();
    // PDS renders the identifier and password in one form. Submitting after
    // only the identifier is disabled; older PDS UIs may split this into two
    // screens, so advance only when there is no password field yet.
    const passwordVisible = await password
      .waitFor({ state: "visible", timeout: 2_000 })
      .then(() => true)
      .catch(() => false);
    if (enteredIdentifier && !passwordVisible) {
      await page.locator('button[type="submit"]').first().click();
    }
    if (
      await password
        .waitFor({ state: "visible", timeout: 8_000 })
        .then(() => true)
        .catch(() => false)
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
      console.info("[verification-e2e] popup: granting consent");
      await authorize.click();
    }

    try {
      await page.waitForURL((url) => url.origin === APP_ORIGIN, { timeout: 5_000 });
    } catch {
      const body = (
        await page
          .locator("body")
          .innerText()
          .catch(() => "")
      ).slice(0, 1_000);
      const url = new URL(page.url());
      throw new Error(
        `verification OAuth did not return to the callback: ${url.origin}${url.pathname}\n${body}`,
      );
    }
    console.info("[verification-e2e] popup: callback returned");
    return page.url();
  } finally {
    assertNoEscape();
  }
}

async function beginConfirmation(
  page: Page,
  label: "Set up verification" | "Remove verification",
  owner: ReturnType<typeof actor>,
): Promise<{ readonly authorizationUrl: string; readonly requestedAfter: number }> {
  const popupCreated = page.context().waitForEvent("page", { timeout: 15_000 });
  const requestedAfter = Date.now();
  console.info(`[verification-e2e] ${label}: opening popup`);
  await page.getByRole("button", { name: label, exact: true }).click();
  const popup = await popupCreated;
  popup.setDefaultTimeout(15_000);
  const authorizationUrl = await authorizePopup(popup, owner);
  console.info(`[verification-e2e] ${label}: waiting for owner confirmation`);
  await expect(page.getByLabel("Owner confirmation")).toBeVisible({ timeout: 30_000 });
  return { authorizationUrl, requestedAfter };
}

async function submitOwnerCode(page: Page, name: string, requestedAfter: number): Promise<void> {
  const owner = await awaitPlcOwnerConfirmation(name, requestedAfter);
  await page.getByLabel("Owner confirmation").fill(owner.code);
  await page.getByRole("button", { name: "Confirm", exact: true }).click();
}

/**
 * Inspect browser storage shape and key names. The browser hashes its standing
 * refresh value locally solely to join the matching PDS row; only that
 * fingerprint and booleans leave the page, never a token, confirmation, or
 * DPoP key.
 */
async function browserIdentityCustody(page: Page): Promise<{
  readonly sessionRows: number;
  readonly sessionFieldsContainIdentity: boolean;
  readonly pendingLogin: boolean;
  readonly operationStorageKeys: readonly string[];
  readonly standingRefreshFingerprint: string | null;
}> {
  return page.evaluate(async () => {
    const values = await new Promise<unknown[]>((resolve, reject) => {
      const request = indexedDB.open("opake");
      request.onerror = () => reject(request.error);
      request.onsuccess = () => {
        const db = request.result;
        if (!db.objectStoreNames.contains("sessions")) {
          db.close();
          resolve([]);
          return;
        }
        const store = db.transaction("sessions", "readonly").objectStore("sessions");
        const rows = store.getAll();
        rows.onerror = () => {
          db.close();
          reject(rows.error);
        };
        rows.onsuccess = () => {
          db.close();
          resolve(rows.result.map((row: { value?: unknown }) => row.value));
        };
      };
    });
    const fields = values.flatMap((value) =>
      value && typeof value === "object" ? Object.keys(value) : [],
    );
    const refresh = values.find(
      (value): value is { refresh_token: string } =>
        !!value &&
        typeof value === "object" &&
        "refresh_token" in value &&
        typeof value.refresh_token === "string",
    )?.refresh_token;
    const digest = refresh
      ? await crypto.subtle.digest("SHA-256", new TextEncoder().encode(refresh))
      : null;
    const operationStorageKeys = [
      ...Array.from({ length: sessionStorage.length }, (_, index) => sessionStorage.key(index)),
      ...Array.from({ length: localStorage.length }, (_, index) => localStorage.key(index)),
    ].filter(
      (key): key is string =>
        typeof key === "string" &&
        /verification|identity.*(?:grant|operation)|pending.*identity/i.test(key),
    );
    return {
      sessionRows: values.length,
      sessionFieldsContainIdentity: fields.some((field) => /identity|grant|operation/i.test(field)),
      pendingLogin: sessionStorage.getItem("opake:pendingLogin") !== null,
      operationStorageKeys,
      standingRefreshFingerprint: digest
        ? Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, "0")).join("")
        : null,
    };
  });
}

test("a namespaced account publishes then removes #opake through real OAuth and PDS confirmation", async ({
  browser,
}) => {
  test.setTimeout(150_000);
  expect(actorNamespace()).not.toBe("");
  const owner = actor("eve");
  const context = await browser.newContext({
    storageState: authFile(owner.name),
    ignoreHTTPSErrors: true,
  });
  const page = await context.newPage();
  page.setDefaultTimeout(15_000);
  const assertNoEscape = await installBlockade(page);
  const revocationPaths: string[] = [];
  page.on("request", (request) => {
    const url = new URL(request.url());
    if (request.method() === "POST" && url.pathname === "/oauth/revoke") {
      revocationPaths.push(url.pathname);
    }
  });

  try {
    await page.goto("/cabinet/settings");
    await expect(
      page.getByRole("button", { name: "Set up verification", exact: true }),
    ).toBeVisible({
      timeout: 30_000,
    });
    const standingBefore = await browserIdentityCustody(page);
    expect(standingBefore.standingRefreshFingerprint).toMatch(/^[0-9a-f]{64}$/);
    const initialScopes = await pdsOAuthScopeSummary(
      owner.name,
      standingBefore.standingRefreshFingerprint!,
    );
    expect(initialScopes.standingTokenFound).toBe(true);
    expect(initialScopes.standingHasIdentityScope).toBe(false);
    const initialLog = await plcOperationLog(owner.name);
    const initialOperation = currentOperation(initialLog);
    expect(currentHasOpake(initialLog)).toBe(false);
    expect(opakeKeyFromDocument(await plcDidDocument(owner.name))).toBeNull();

    // A bad owner code reaches the real PDS signer and is rejected there. It
    // must not publish a DID method or make the next fresh grant resumable.
    const rejected = await beginConfirmation(page, "Set up verification", owner);
    await page.getByLabel("Owner confirmation").fill("NOT-A-VALID-OWNER-CODE");
    await page.getByRole("button", { name: "Confirm", exact: true }).click();
    await expect(
      page.getByRole("button", { name: "Set up verification", exact: true }),
    ).toBeVisible({ timeout: 30_000 });
    expect(currentHasOpake(await plcOperationLog(owner.name))).toBe(false);
    expect(opakeKeyFromDocument(await plcDidDocument(owner.name))).toBeNull();
    const scopesAfterRejected = await pdsOAuthScopeSummary(
      owner.name,
      standingBefore.standingRefreshFingerprint!,
    );
    expect(scopesAfterRejected.standingTokenFound).toBe(true);
    expect(scopesAfterRejected.standingHasIdentityScope).toBe(false);

    // `startVerificationMethodPublication` writes the signed PDS key record
    // using the standing session before it can ask PDS to sign a PLC update.
    // Observe that durable ordering while the real operation is stopped at the
    // confirmation gate: the PDS commit has advanced and is signed, but the
    // PLC log still has no #opake method.
    const commitBefore = await repositoryCommit(owner);
    const setup = await beginConfirmation(page, "Set up verification", owner);
    const custodyDuringSetup = await browserIdentityCustody(page);
    expect(custodyDuringSetup.sessionRows).toBe(1);
    expect(custodyDuringSetup.sessionFieldsContainIdentity).toBe(false);
    expect(custodyDuringSetup.pendingLogin).toBe(false);
    expect(custodyDuringSetup.operationStorageKeys).toEqual([]);
    expect(custodyDuringSetup.standingRefreshFingerprint).toBe(
      standingBefore.standingRefreshFingerprint,
    );
    // The PDS has issued the live operation's identity authority, proving the
    // aggregate scope oracle can observe it before later proving cleanup.
    const scopesDuringSetup = await pdsOAuthScopeSummary(
      owner.name,
      custodyDuringSetup.standingRefreshFingerprint!,
    );
    expect(scopesDuringSetup.identityTokenCount).toBeGreaterThan(
      scopesAfterRejected.identityTokenCount,
    );
    expect(scopesDuringSetup.standingTokenFound).toBe(true);
    expect(scopesDuringSetup.standingHasIdentityScope).toBe(false);
    expect(setup.authorizationUrl).not.toBe(rejected.authorizationUrl);
    const record = await repositoryRecord(owner, PUBLIC_KEY_COLLECTION, PUBLIC_KEY_RKEY);
    expect(record?.value).toMatchObject({
      signingAlgo: "ed25519",
      signatureAlgo: "ed25519",
    });
    expect(await repositoryCommit(owner)).not.toBe(commitBefore);
    const preConfirmationLog = await plcOperationLog(owner.name);
    expect(currentHasOpake(preConfirmationLog)).toBe(false);
    expect(opakeKeyFromDocument(await plcDidDocument(owner.name))).toBeNull();

    await submitOwnerCode(page, owner.name, setup.requestedAfter);
    await expect(
      page.getByRole("button", { name: "Remove verification", exact: true }),
    ).toBeVisible({
      timeout: 30_000,
    });
    const publishedLog = await plcOperationLog(owner.name);
    const publishedOperation = currentOperation(publishedLog);
    const publicKey = expectedSigningKey(record?.value);
    const methods = publishedOperation.verificationMethods as Record<string, unknown>;
    expect(methods.opake).toEqual(expect.any(String));
    expect(plcPublicKey(methods.opake)).toEqual(publicKey);
    const initialMethods = initialOperation.verificationMethods as Record<string, unknown>;
    const { opake: _initialOpake, ...initialUnrelatedMethods } = initialMethods;
    const { opake: _publishedOpake, ...publishedUnrelatedMethods } = methods;
    expect(publishedUnrelatedMethods).toEqual(initialUnrelatedMethods);
    for (const field of ["rotationKeys", "alsoKnownAs", "services"]) {
      expect(publishedOperation[field]).toEqual(initialOperation[field]);
    }
    expect(opakeKeyFromDocument(await plcDidDocument(owner.name))).toEqual(publicKey);

    const removal = await beginConfirmation(page, "Remove verification", owner);
    const submitted = page.getByText("Verification operation was submitted.").first();
    const revocationsBeforeRemoval = revocationPaths.length;
    await submitOwnerCode(page, owner.name, removal.requestedAfter);
    await expect(submitted).toBeVisible({ timeout: 30_000 });
    expect(revocationPaths.slice(revocationsBeforeRemoval)).toEqual([
      "/oauth/revoke",
      "/oauth/revoke",
    ]);
    await expect(
      page.getByRole("button", { name: "Set up verification", exact: true }),
    ).toBeVisible({
      timeout: 30_000,
    });
    const removedLog = await plcOperationLog(owner.name);
    const removedOperation = currentOperation(removedLog);
    expect(currentHasOpake(removedLog)).toBe(false);
    expect(removedOperation.verificationMethods).toEqual(initialOperation.verificationMethods);
    for (const field of ["rotationKeys", "alsoKnownAs", "services"]) {
      expect(removedOperation[field]).toEqual(initialOperation[field]);
    }
    expect(opakeKeyFromDocument(await plcDidDocument(owner.name))).toBeNull();

    // The UI reports submission, never a server-revocation guarantee. This
    // aggregate-only PDS read independently shows why: the
    // provider can retain the identity grant after handling its endpoint.
    const scopesAfterRemoval = await pdsOAuthScopeSummary(
      owner.name,
      standingBefore.standingRefreshFingerprint!,
    );
    expect(scopesAfterRemoval.tokenCount).toBeGreaterThan(0);
    expect(scopesAfterRemoval.hasIdentityScope).toBe(true);
    expect(scopesAfterRemoval.standingTokenFound).toBe(true);
    expect(scopesAfterRemoval.standingHasIdentityScope).toBe(false);

    // The standing session was not consumed by any identity grant. A real,
    // ordinary browser repository mutation after cleanup is stronger evidence
    // than a cached cabinet route: it waits for the PDS write response.
    await page.goto("/cabinet/files");
    await expect(page).not.toHaveURL(/\/devices\/login/, { timeout: 30_000 });
    await gotoCabinetRoot(page);
    const ordinaryMutationCommit = await repositoryCommit(owner);
    const filename = `verification-standing-${Date.now().toString(36)}.txt`;
    await uploadFile(page, filename, "standing session remains usable");
    await expect(page.getByText("File uploaded").first()).toBeVisible({ timeout: 60_000 });
    expect(await repositoryCommit(owner)).not.toBe(ordinaryMutationCommit);

    // An abrupt cancellation/reload makes no promise about server revocation.
    // It does prove the live holder and owner confirmation never become a
    // stored continuation: the fresh page is absent and has no input to resume.
    await page.goto("/cabinet/settings");
    await beginConfirmation(page, "Set up verification", owner);
    await page.getByRole("button", { name: "Cancel verification", exact: true }).click();
    await page.reload();
    await expect(
      page.getByRole("button", { name: "Set up verification", exact: true }),
    ).toBeVisible({ timeout: 30_000 });
    await expect(page.getByLabel("Owner confirmation")).toHaveCount(0);
    expect(currentHasOpake(await plcOperationLog(owner.name))).toBe(false);
    expect(opakeKeyFromDocument(await plcDidDocument(owner.name))).toBeNull();
    assertNoEscape();
  } finally {
    await context.close().catch(() => undefined);
  }
});
