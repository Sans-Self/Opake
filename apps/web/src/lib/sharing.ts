// Stubbed — sharing not yet implemented

export class RecipientNotReadyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "RecipientNotReadyError";
  }
}

export async function resolveRecipient(_handle: string) {
  throw new Error("Sharing not implemented");
}
