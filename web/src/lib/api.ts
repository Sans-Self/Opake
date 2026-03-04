interface ApiConfig {
  pdsUrl: string;
  appviewUrl: string;
}

const defaultConfig: ApiConfig = {
  pdsUrl: import.meta.env.VITE_PDS_URL ?? "https://pds.sans-self.org",
  appviewUrl: import.meta.env.VITE_APPVIEW_URL ?? "https://appview.opake.app",
};

interface XrpcParams {
  lexicon: string;
  method?: "GET" | "POST";
  body?: unknown;
  headers?: Record<string, string>;
}

export async function xrpc(
  params: XrpcParams,
  config: ApiConfig = defaultConfig,
): Promise<unknown> {
  const { lexicon, method = "GET", body, headers = {} } = params;
  const url = `${config.pdsUrl}/xrpc/${lexicon}`;

  const response = await fetch(url, {
    method,
    headers: {
      "Content-Type": "application/json",
      ...headers,
    },
    body: body ? JSON.stringify(body) : undefined,
  });

  if (!response.ok) {
    throw new Error(`XRPC ${lexicon}: ${response.status}`);
  }

  return response.json();
}

export async function appview(
  path: string,
  config: ApiConfig = defaultConfig,
): Promise<unknown> {
  const response = await fetch(`${config.appviewUrl}${path}`);

  if (!response.ok) {
    throw new Error(`AppView ${path}: ${response.status}`);
  }

  return response.json();
}
