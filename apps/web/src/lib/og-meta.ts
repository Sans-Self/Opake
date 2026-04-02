const SITE_URL = (import.meta.env.VITE_SITE_URL as string | undefined) ?? "https://opake.app";

interface OgMeta {
  readonly title: string;
  readonly description: string;
  readonly image?: string;
}

export function ogMeta({ title, description, image }: OgMeta) {
  const ogImage = image ? `${SITE_URL}${image}` : `${SITE_URL}/og/default.png`;

  return [
    { title },
    { name: "description", content: description },
    { property: "og:title", content: title },
    { property: "og:description", content: description },
    { property: "og:image", content: ogImage },
    { name: "twitter:title", content: title },
    { name: "twitter:description", content: description },
    { name: "twitter:image", content: ogImage },
    { name: "twitter:card", content: "summary_large_image" },
  ];
}
