// Code snippets for the /understand/ docs pages.
// See apps/web/src/content/docs/build/sdk/_snippets.ts for the rationale
// (MDX 3 dedents template literals inside .mdx files; imports bypass it).

export const encryptBlob = `// The core primitive.
pub fn encrypt_blob(
    plaintext: &[u8],
    key: &ContentKey
) -> Result<Vec<u8>, CryptoError> {
    // Generates 12-byte random nonce
    // Applies AES-GCM
    // Returns ciphertext with appended authentication tag
}`;
