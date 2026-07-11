export interface PublicOrigins {
  listingOrigin: string;
  imageOrigin: string;
}

export const PRODUCTION_PUBLIC_ORIGINS: Readonly<PublicOrigins> = Object.freeze(
  {
    imageOrigin: "https://images.budna.se",
    listingOrigin: "https://budna.se",
  },
);

const CONFIG_ELEMENT_ID = "budna-mcp-runtime-config";

export function readPublicOrigins(
  document_: Document = document,
): PublicOrigins {
  const element = document_.getElementById(CONFIG_ELEMENT_ID);
  if (!element) return { ...PRODUCTION_PUBLIC_ORIGINS };

  try {
    const parsed = JSON.parse(element.textContent ?? "") as unknown;
    const value = asObject(parsed);
    return {
      imageOrigin:
        validHttpsOrigin(value?.image_origin) ??
        PRODUCTION_PUBLIC_ORIGINS.imageOrigin,
      listingOrigin:
        validHttpsOrigin(value?.listing_origin) ??
        PRODUCTION_PUBLIC_ORIGINS.listingOrigin,
    };
  } catch {
    return { ...PRODUCTION_PUBLIC_ORIGINS };
  }
}

function validHttpsOrigin(value: unknown): string | undefined {
  if (typeof value !== "string" || value.length === 0 || value.length > 2_048) {
    return undefined;
  }
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.username !== "" ||
      url.password !== "" ||
      url.pathname !== "/" ||
      url.search !== "" ||
      url.hash !== "" ||
      url.hostname.includes("*") ||
      url.origin === "null"
    ) {
      return undefined;
    }
    return url.origin;
  } catch {
    return undefined;
  }
}

function asObject(value: unknown): Record<string, unknown> | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : undefined;
}
