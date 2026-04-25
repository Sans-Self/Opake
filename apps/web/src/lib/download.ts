// Browser download helper — trigger a file save from in-memory data.

export function triggerBrowserDownload(data: Uint8Array, filename: string, mimeType: string): void {
  const buffer = new ArrayBuffer(data.byteLength);
  new Uint8Array(buffer).set(data);
  const blob = new Blob([buffer], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  // eslint-disable-next-line functional/immutable-data -- DOM side effect at system edge
  anchor.href = url;
  // eslint-disable-next-line functional/immutable-data -- DOM side effect at system edge
  anchor.download = filename;
  anchor.click();
  URL.revokeObjectURL(url);
}
