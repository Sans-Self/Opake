// Interactive login helper for e2e tests.
//
// Handles the seed phrase confirmation flow by parsing the mnemonic grid
// from stdout and responding to word prompts automatically.

import { opakeInteractive, type CliResult } from "./cli.js";
import { getPds } from "./pds.js";

/** Parse numbered word entries from the mnemonic grid in stdout. */
export function parseSeedPhrase(output: string): string {
  const words: string[] = new Array(24).fill("");
  const matches = output.matchAll(/(\d+)\.\s+(\S+)/g);
  for (const match of matches) {
    const index = parseInt(match[1]!, 10) - 1;
    if (index >= 0 && index < 24) {
      words[index] = match[2]!;
    }
  }
  return words.join(" ");
}

interface LoginResult extends CliResult {
  readonly seedPhrase: string;
}

/**
 * Perform an interactive legacy login against the fake PDS.
 * Handles password (via env var), seed phrase confirmation (3 words),
 * and seed file save prompt.
 *
 * @param handle - Account handle (e.g. "alice.test")
 * @param configDir - CLI config directory
 * @param saveSeedTo - Path to save the seed phrase grid file, or "skip"
 */
export async function interactiveLogin(
  handle: string,
  configDir: string,
  saveSeedTo: string = "skip",
): Promise<LoginResult> {
  const pds = getPds();
  const answered = new Set<string>();
  // eslint-disable-next-line functional/no-let
  let capturedPhrase = "";

  const result = await opakeInteractive(
    ["account", "login", handle, "--legacy", "--pds", pds.url],
    {
      configDir,
      env: { OPAKE_CLI_PASSWORD: "test" },
      respond(output) {
        if (output.includes("Your seed phrase") && !capturedPhrase) {
          capturedPhrase = parseSeedPhrase(output);
        }

        const wordPrompt = output.match(/Enter word #(\d+):/g);
        if (wordPrompt) {
          const words = capturedPhrase.split(" ");
          for (const prompt of wordPrompt) {
            if (answered.has(prompt)) continue;
            const num = parseInt(prompt.match(/(\d+)/)![1]!, 10);
            const word = words[num - 1];
            if (word) {
              answered.add(prompt);
              return word;
            }
          }
        }

        if (output.includes("Save seed phrase") && !answered.has("save")) {
          answered.add("save");
          return saveSeedTo;
        }

        return null;
      },
      responseDelay: 100,
    },
  );

  return { ...result, seedPhrase: capturedPhrase };
}
