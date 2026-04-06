import { createLazyFileRoute } from "@tanstack/react-router";
import { FileView } from "@/components/cabinet/FileView";

function FilesPath() {
  const { _splat } = Route.useParams();
  const segments = (_splat ?? "").split("/").filter(Boolean);

  return (
    <FileView
      rootLabel="Your Cabinet"
      pathSegments={segments}
      context={{ kind: "cabinet" }}
      basePath="/cabinet/files"
    />
  );
}

export const Route = createLazyFileRoute("/cabinet/files/$")({
  component: FilesPath,
});
