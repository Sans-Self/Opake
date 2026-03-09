import { useEffect, useState } from "react";
import Lightbox from "yet-another-react-lightbox";
import Inline from "yet-another-react-lightbox/plugins/inline";
import Zoom from "yet-another-react-lightbox/plugins/zoom";
import { MagnifyingGlassPlusIcon, MagnifyingGlassMinusIcon } from "@phosphor-icons/react";
import "yet-another-react-lightbox/styles.css";
import { OpakeLogoSquares } from "@/components/OpakeLogoSquares";
import { resizeImageBlob } from "@/lib/resizeImage";

interface ImagePreviewProps {
  readonly data: Uint8Array;
  readonly mimeType: string;
}

export function ImagePreview({ data, mimeType }: ImagePreviewProps) {
  const [blobUrl, setBlobUrl] = useState<string | null>(null);

  useEffect(() => {
    // eslint-disable-next-line functional/no-let -- mutable cleanup flag for async lifecycle
    let revoked = false;
    // eslint-disable-next-line functional/no-let -- tracks URL for cleanup
    let url: string | null = null;

    void resizeImageBlob(data, mimeType).then((resizedUrl) => {
      if (revoked) {
        URL.revokeObjectURL(resizedUrl);
        return;
      }
      url = resizedUrl;
      setBlobUrl(resizedUrl);
    });

    return () => {
      revoked = true;
      if (url) URL.revokeObjectURL(url);
    };
  }, [data, mimeType]);

  if (!blobUrl) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <OpakeLogoSquares size="lg" loading />
      </div>
    );
  }

  return (
    <div className="relative h-full w-full overflow-hidden">
      <Lightbox
        open
        close={() => undefined}
        slides={[{ src: blobUrl }]}
        plugins={[Inline, Zoom]}
        inline={{ style: { width: "100%", height: "100%" } }}
        carousel={{ finite: true }}
        render={{
          buttonPrev: () => null,
          buttonNext: () => null,
          buttonClose: () => null,
          iconZoomIn: () => (
            <button className="btn btn-sm btn-square" aria-label="Zoom in">
              <MagnifyingGlassPlusIcon size={16} />
            </button>
          ),
          iconZoomOut: () => (
            <button className="btn btn-sm btn-square" aria-label="Zoom out">
              <MagnifyingGlassMinusIcon size={16} />
            </button>
          ),
        }}
        zoom={{ maxZoomPixelRatio: 4 }}
        styles={{
          container: { backgroundColor: "transparent" },
          slide: { alignItems: "flex-start", padding: "1rem" },
        }}
      />
    </div>
  );
}
