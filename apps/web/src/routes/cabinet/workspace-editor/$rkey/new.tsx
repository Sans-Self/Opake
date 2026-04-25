import { createFileRoute } from "@tanstack/react-router";
import { z } from "zod";

const searchSchema = z.object({
  directoryUri: z.string().optional(),
});

export const Route = createFileRoute("/cabinet/workspace-editor/$rkey/new")({
  validateSearch: searchSchema,
});
