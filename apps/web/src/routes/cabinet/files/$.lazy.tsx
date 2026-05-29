import { createLazyFileRoute } from "@tanstack/react-router";
import { FileView } from "@/components/cabinet/FileView";
import { parseSplatPath } from "@/lib/namePath";

function FilesPath() {
  const { _splat } = Route.useParams();
  const { dirSegments, fileSegment } = parseSplatPath(_splat);

  return (
    <FileView
      rootLabel="Your Cabinet"
      pathSegments={dirSegments}
      fileSegment={fileSegment}
      context={{ kind: "cabinet" }}
      basePath="/cabinet/files"
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/files/$")({
  component: FilesPath,
});
