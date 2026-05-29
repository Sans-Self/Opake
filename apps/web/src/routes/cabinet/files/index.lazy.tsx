import { createLazyFileRoute } from "@tanstack/react-router";
import { FileView } from "@/components/cabinet/FileView";

function FilesIndex() {
  return (
    <FileView
      rootLabel="Your Cabinet"
      pathSegments={[]}
      fileSegment={null}
      context={{ kind: "cabinet" }}
      basePath="/cabinet/files"
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/files/")({
  component: FilesIndex,
});
