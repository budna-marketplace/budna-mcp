// @vitest-environment jsdom

import { afterEach, describe, expect, it } from "vitest";
import {
  PRODUCTION_PUBLIC_ORIGINS,
  readPublicOrigins,
} from "@budna-ui/runtime-config";

afterEach(() => document.body.replaceChildren());

describe("public origin runtime configuration", () => {
  it("reads alternate HTTPS origins from the inert JSON element", () => {
    installConfig({
      image_origin: "https://media.example.com",
      listing_origin: "https://market.example.com",
    });
    expect(readPublicOrigins()).toEqual({
      imageOrigin: "https://media.example.com",
      listingOrigin: "https://market.example.com",
    });
  });

  it("falls back independently for malformed or non-origin values", () => {
    installConfig({
      image_origin: "https://media.example.com",
      listing_origin: "http://market.example.com",
    });
    expect(readPublicOrigins()).toEqual({
      imageOrigin: "https://media.example.com",
      listingOrigin: PRODUCTION_PUBLIC_ORIGINS.listingOrigin,
    });

    installRawConfig("__BUDNA_MCP_PUBLIC_ORIGINS_JSON__");
    expect(readPublicOrigins()).toEqual(PRODUCTION_PUBLIC_ORIGINS);
  });

  it("rejects credentials, queries, and fragments", () => {
    installConfig({
      image_origin: "https://user@media.example.com",
      listing_origin: "https://market.example.com?mode=public#top",
    });
    expect(readPublicOrigins()).toEqual(PRODUCTION_PUBLIC_ORIGINS);

    installConfig({
      image_origin: "https://*.example.com",
      listing_origin: "https://market.example.com/path",
    });
    expect(readPublicOrigins()).toEqual(PRODUCTION_PUBLIC_ORIGINS);
  });
});

function installConfig(config: Record<string, unknown>): void {
  installRawConfig(JSON.stringify(config));
}

function installRawConfig(value: string): void {
  document.getElementById("budna-mcp-runtime-config")?.remove();
  const element = document.createElement("script");
  element.id = "budna-mcp-runtime-config";
  element.type = "application/json";
  element.textContent = value;
  document.body.append(element);
}
