// Canvas-based image resize — caps to a max dimension, returns a blob URL.
// SVGs are skipped (vector, no resize needed).

const MAX_DIMENSION = 2000;

export async function resizeImageBlob(
  data: Uint8Array,
  mimeType: string,
  maxDimension: number = MAX_DIMENSION,
): Promise<string> {
  const buffer = new ArrayBuffer(data.byteLength);
  new Uint8Array(buffer).set(data);
  const sourceUrl = URL.createObjectURL(new Blob([buffer], { type: mimeType }));

  // Vector images don't need rasterized resizing
  if (mimeType === "image/svg+xml") return sourceUrl;

  try {
    const image = await loadImage(sourceUrl);
    const { width, height } = image;

    // Already within bounds — return the source blob URL directly
    if (width <= maxDimension && height <= maxDimension) return sourceUrl;

    const scale = maxDimension / Math.max(width, height);
    const targetWidth = Math.round(width * scale);
    const targetHeight = Math.round(height * scale);

    const canvas = document.createElement("canvas");
    // eslint-disable-next-line functional/immutable-data -- canvas setup
    canvas.width = targetWidth;
    // eslint-disable-next-line functional/immutable-data -- canvas setup
    canvas.height = targetHeight;

    const ctx = canvas.getContext("2d");
    if (!ctx) throw new Error("Canvas 2D context unavailable");

    ctx.drawImage(image, 0, 0, targetWidth, targetHeight);

    const resizedBlob = await new Promise<Blob>((resolve, reject) => {
      canvas.toBlob(
        (blob) => (blob ? resolve(blob) : reject(new Error("Canvas toBlob failed"))),
        mimeType,
        0.92,
      );
    });

    // Release the original blob URL — we have a resized one now
    URL.revokeObjectURL(sourceUrl);
    return URL.createObjectURL(resizedBlob);
  } catch {
    // Fallback: return the original blob URL if resize fails
    return sourceUrl;
  }
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    // eslint-disable-next-line functional/immutable-data -- event handler setup
    img.onload = () => resolve(img);
    // eslint-disable-next-line functional/immutable-data -- event handler setup
    img.onerror = () => reject(new Error("Failed to load image"));
    // eslint-disable-next-line functional/immutable-data -- property assignment
    img.src = src;
  });
}
