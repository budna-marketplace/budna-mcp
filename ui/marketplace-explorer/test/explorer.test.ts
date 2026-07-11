// @vitest-environment jsdom

import axe from "axe-core";
import type {
  McpUiDisplayMode,
  McpUiHostCapabilities,
  McpUiHostContext,
} from "@modelcontextprotocol/ext-apps";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MarketplaceExplorer, type HostBridge } from "@budna-ui/explorer";
import { detailResult, listing, searchResult } from "./fixtures";

class FakeBridge implements HostBridge {
  calls: Array<{ arguments_: Record<string, unknown>; name: string }> = [];
  opened: string[] = [];
  messages: string[] = [];
  contexts: Record<string, unknown>[] = [];
  failContext = false;
  failMessage = false;
  nextResult: CallToolResult = detailResult();
  openResult: { isError?: boolean } = {};
  hostCapabilities: McpUiHostCapabilities = {
    message: { text: {} },
    openLinks: {},
    serverTools: {},
    updateModelContext: { structuredContent: {} },
  };
  hostContext: McpUiHostContext = {
    availableDisplayModes: ["inline", "fullscreen"],
    displayMode: "inline",
    locale: "en-US",
    timeZone: "Europe/Oslo",
    toolInfo: {
      tool: { inputSchema: { type: "object" }, name: "search_listings" },
    },
  };

  async callTool(
    name: string,
    arguments_: Record<string, unknown>,
  ): Promise<CallToolResult> {
    this.calls.push({ arguments_, name });
    return this.nextResult;
  }

  capabilities(): McpUiHostCapabilities {
    return this.hostCapabilities;
  }

  context(): McpUiHostContext {
    return this.hostContext;
  }

  async openLink(url: string): Promise<{ isError?: boolean }> {
    this.opened.push(url);
    return this.openResult;
  }

  async requestDisplayMode(mode: McpUiDisplayMode): Promise<McpUiDisplayMode> {
    return mode;
  }

  async sendMessage(text: string): Promise<{ isError?: boolean }> {
    if (this.failMessage) throw new Error("Synthetic rejection");
    this.messages.push(text);
    return {};
  }

  async updateModelContext(
    structuredContent: Record<string, unknown>,
  ): Promise<void> {
    if (this.failContext) throw new Error("Synthetic rejection");
    this.contexts.push(structuredContent);
  }
}

afterEach(() => {
  document.body.replaceChildren();
  document.documentElement.removeAttribute("data-theme");
});

function renderSearch(
  result: CallToolResult = searchResult(),
  bridge = new FakeBridge(),
): { bridge: FakeBridge; explorer: MarketplaceExplorer; root: HTMLElement } {
  const root = document.createElement("div");
  document.body.append(root);
  const explorer = new MarketplaceExplorer(root, bridge);
  explorer.receiveToolInput({ query: "camera" });
  explorer.connected();
  explorer.receiveToolResult(result);
  return { bridge, explorer, root };
}

describe("MarketplaceExplorer", () => {
  it("renders hostile marketplace text as text, never as markup", () => {
    const hostile = '<img data-hostile="true" src=x>Hej 👋';
    const { root } = renderSearch(
      searchResult([listing(1, { title: hostile })]),
    );

    expect(root.textContent).toContain(hostile);
    expect(root.querySelector("[data-hostile]")).toBeNull();
  });

  it("opens listing details through the existing read-only tool", async () => {
    const { bridge, root } = renderSearch();
    const detail = root.querySelector<HTMLButtonElement>(".title-button");
    detail?.click();

    await vi.waitFor(() => expect(bridge.calls[0]?.name).toBe("get_listing"));
    await vi.waitFor(() =>
      expect(root.textContent).toContain("A carefully used camera."),
    );
    expect(bridge.calls[0]?.arguments_).toEqual({ listing_id: 1 });
  });

  it("uses a focusable native button for keyboard listing activation", () => {
    const { root } = renderSearch();
    const detail = root.querySelector<HTMLButtonElement>(".title-button");
    expect(detail?.tagName).toBe("BUTTON");
    expect(detail?.disabled).toBe(false);
    detail?.focus();
    expect(document.activeElement).toBe(detail);
  });

  it("opens only an attributed Budna URL through the host", async () => {
    const { bridge, root } = renderSearch();
    const open = [...root.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent === "View on Budna",
    );
    open?.click();

    await vi.waitFor(() => expect(bridge.opened).toHaveLength(1));
    expect(bridge.opened[0]).toContain("https://budna.se/l/1?");
    expect(bridge.opened[0]).toContain("utm_campaign=interactive_cards");
  });

  it("shows a recoverable message when the host rejects an external link", async () => {
    const bridge = new FakeBridge();
    bridge.openResult = { isError: true };
    const { root } = renderSearch(searchResult(), bridge);
    const open = [...root.querySelectorAll<HTMLButtonElement>("button")].find(
      (button) => button.textContent === "View on Budna",
    );
    open?.click();

    await vi.waitFor(() =>
      expect(root.textContent).toContain("did not allow this link"),
    );
    expect(bridge.opened).toHaveLength(1);
  });

  it("keeps comparison selections when the host lacks context capabilities", async () => {
    const bridge = new FakeBridge();
    bridge.hostCapabilities = { openLinks: {}, serverTools: {} };
    const { root } = renderSearch(searchResult(), bridge);
    const checkboxes = [
      ...root.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
    ];
    checkboxes[0]?.click();
    root
      .querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1]
      ?.click();
    const compare = [
      ...root.querySelectorAll<HTMLButtonElement>("button"),
    ].find((button) => button.textContent === "Ask AI to compare");
    compare?.click();

    await vi.waitFor(() =>
      expect(root.textContent).toContain("does not support comparison"),
    );
    expect(
      [
        ...root.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
      ].filter((checkbox) => checkbox.checked),
    ).toHaveLength(2);
  });

  it("sends a bounded comparison context followed by a user message", async () => {
    const { bridge, root } = renderSearch();
    root
      .querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[0]
      ?.click();
    root
      .querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1]
      ?.click();
    const compare = [
      ...root.querySelectorAll<HTMLButtonElement>("button"),
    ].find((button) => button.textContent === "Ask AI to compare");
    compare?.click();

    await vi.waitFor(() => expect(bridge.messages).toHaveLength(1));
    expect(bridge.contexts[0]).toMatchObject({
      listings: [{ id: 1 }, { id: 2 }],
      type: "budna_listing_comparison",
      version: "1",
    });
    expect(root.textContent).toContain("sent to the conversation");
  });

  it.each(["context", "message"] as const)(
    "preserves selection when the host rejects the comparison %s request",
    async (failure) => {
      const bridge = new FakeBridge();
      bridge.failContext = failure === "context";
      bridge.failMessage = failure === "message";
      const { root } = renderSearch(searchResult(), bridge);
      root
        .querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[0]
        ?.click();
      root
        .querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1]
        ?.click();
      const compare = [
        ...root.querySelectorAll<HTMLButtonElement>("button"),
      ].find((button) => button.textContent === "Ask AI to compare");
      compare?.click();

      await vi.waitFor(() =>
        expect(root.textContent).toContain("does not support comparison"),
      );
      expect(
        [
          ...root.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'),
        ].filter((checkbox) => checkbox.checked),
      ).toHaveLength(2);
    },
  );

  it("loads more with deduplication and a hard 50-listing ceiling", async () => {
    const bridge = new FakeBridge();
    const initial = Array.from({ length: 49 }, (_, index) =>
      listing(index + 1),
    );
    bridge.nextResult = searchResult(
      [listing(49), listing(50), listing(51)],
      2,
      2,
    );
    const { root } = renderSearch(searchResult(initial, 1, 2), bridge);
    expect(root.querySelectorAll(".listing-card")).toHaveLength(49);
    const loadMore = [
      ...root.querySelectorAll<HTMLButtonElement>("button"),
    ].find((button) => button.textContent === "Load more");
    loadMore?.click();

    await vi.waitFor(() =>
      expect(root.querySelectorAll(".listing-card")).toHaveLength(50),
    );
    expect(bridge.calls[0]).toMatchObject({
      arguments_: { limit: 1, page: 2, query: "camera" },
      name: "search_listings",
    });
    expect(root.textContent).not.toContain("Camera 51");

    bridge.nextResult = searchResult(initial.slice(0, 2), 1, 2);
    const refresh = [
      ...root.querySelectorAll<HTMLButtonElement>("button"),
    ].find((button) => button.textContent === "Refresh");
    refresh?.click();
    await vi.waitFor(() => expect(bridge.calls).toHaveLength(2));
    expect(bridge.calls[1]).toEqual({
      arguments_: { query: "camera" },
      name: "search_listings",
    });
  });

  it("refreshes selected listing data before sending a comparison", async () => {
    const { bridge, root } = renderSearch();
    root
      .querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[0]
      ?.click();
    root
      .querySelectorAll<HTMLInputElement>('input[type="checkbox"]')[1]
      ?.click();
    bridge.nextResult = searchResult([
      listing(1, { current_bid: { amount: "1777.00", currency_code: "NOK" } }),
      listing(2),
    ]);
    const refresh = [
      ...root.querySelectorAll<HTMLButtonElement>("button"),
    ].find((button) => button.textContent === "Refresh");
    refresh?.click();
    await vi.waitFor(() => expect(bridge.calls).toHaveLength(1));
    const compare = [
      ...root.querySelectorAll<HTMLButtonElement>("button"),
    ].find((button) => button.textContent === "Ask AI to compare");
    compare?.click();
    await vi.waitFor(() => expect(bridge.contexts).toHaveLength(1));
    expect(bridge.contexts[0]).toMatchObject({
      listings: [{ id: 1, price: { amount: "1777.00" } }, { id: 2 }],
    });
  });

  it("uses host locale, theme, timezone, and safe-area context", () => {
    const bridge = new FakeBridge();
    bridge.hostContext = {
      ...bridge.hostContext,
      locale: "sv-SE",
      safeAreaInsets: { bottom: 12, left: 3, right: 4, top: 8 },
      theme: "dark",
    };
    const { root } = renderSearch(searchResult(), bridge);

    expect(root.textContent).toContain("Sökresultat");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(root.style.getPropertyValue("--safe-bottom")).toBe("12px");
    expect(document.documentElement.lang).toBe("sv");
  });

  it("renders Norwegian UI for Bokmål and Nynorsk host locales", () => {
    for (const locale of ["nb-NO", "nn-NO"]) {
      document.body.replaceChildren();
      const bridge = new FakeBridge();
      bridge.hostContext = { ...bridge.hostContext, locale };
      const { root } = renderSearch(searchResult(), bridge);
      expect(root.textContent).toContain("Søkeresultater");
      expect(root.textContent).toContain("Oppdater");
      expect(document.documentElement.lang).toBe("nb");
    }
  });

  it("renders missing and broken image states without unsafe fallbacks", () => {
    const missing = renderSearch(
      searchResult([listing(1, { image_urls: [], primary_image_url: null })]),
    );
    expect(missing.root.textContent).toContain("Image unavailable");
    document.body.replaceChildren();
    const broken = renderSearch();
    const image =
      broken.root.querySelector<HTMLImageElement>(".card-image img");
    const frame = image?.closest(".card-image");
    expect(image?.referrerPolicy).toBe("no-referrer");
    image?.dispatchEvent(new Event("error"));
    expect(broken.root.textContent).toContain("Image unavailable");
    expect(frame?.querySelector("img")).toBeNull();
  });

  it("shows neutral bid availability without inventing a count", () => {
    const hasBids = renderSearch(
      searchResult([listing(1, { bid_count: null, has_bids: true })]),
    );
    expect(hasBids.root.textContent).toContain("Has bids");
    expect(hasBids.root.textContent).not.toContain("1 bid");
    document.body.replaceChildren();
    const noBids = renderSearch(
      searchResult([listing(1, { bid_count: null, has_bids: false })]),
    );
    expect(noBids.root.textContent).toContain("No bids");
  });

  it("renders an empty state and a recoverable malformed-result state", () => {
    const empty = renderSearch(searchResult([]));
    expect(empty.root.textContent).toContain("No listings were found");
    document.body.replaceChildren();
    const malformed = renderSearch({
      content: [{ text: "{}", type: "text" }],
      structuredContent: {},
    });
    expect(malformed.root.textContent).toContain("could not safely display");
  });

  it("renders a recoverable tool-error state", () => {
    const { root } = renderSearch({
      content: [{ text: "synthetic failure", type: "text" }],
      isError: true,
    });
    expect(root.textContent).toContain("could not load these listings");
    expect(root.textContent).not.toContain("could not safely display");
  });

  it("has no detectable automated accessibility violations", async () => {
    renderSearch();
    const report = await axe.run(document.body, {
      rules: { "color-contrast": { enabled: false } },
    });
    expect(report.violations).toEqual([]);
  });

  it("has no detectable accessibility violations in the detail gallery", async () => {
    const bridge = new FakeBridge();
    bridge.hostContext = {
      ...bridge.hostContext,
      toolInfo: {
        tool: { inputSchema: { type: "object" }, name: "get_listing" },
      },
    };
    const firstImage =
      "https://images.budna.se/t/listings/1/thumbs/0190cafe-0000-7000-8000-000000000001_768x768.webp";
    const secondImage =
      "https://images.budna.se/t/listings/1/thumbs/0190cafe-0000-7000-8000-000000000002_768x768.webp";
    const root = document.createElement("div");
    document.body.append(root);
    const explorer = new MarketplaceExplorer(root, bridge);
    explorer.receiveToolInput({ listing_id: 1 });
    explorer.connected();
    explorer.receiveToolResult(
      detailResult(1, {
        image_urls: [firstImage, secondImage],
        primary_image_url: firstImage,
      }),
    );
    expect(root.querySelectorAll(".thumbnail-button")).toHaveLength(2);

    const initialMain =
      root.querySelector<HTMLImageElement>(".detail-image img");
    expect(initialMain?.referrerPolicy).toBe("no-referrer");
    initialMain?.dispatchEvent(new Event("error"));
    expect(root.querySelector(".detail-image .image-fallback")).not.toBeNull();
    const thumbnails = [
      ...root.querySelectorAll<HTMLButtonElement>(".thumbnail-button"),
    ];
    thumbnails[1]?.click();
    expect(root.querySelector<HTMLImageElement>(".detail-image img")?.src).toBe(
      secondImage,
    );
    const brokenThumbnail =
      thumbnails[0]?.querySelector<HTMLImageElement>("img");
    expect(brokenThumbnail?.referrerPolicy).toBe("no-referrer");
    brokenThumbnail?.dispatchEvent(new Event("error"));
    expect(thumbnails[0]?.disabled).toBe(true);
    expect(thumbnails[0]?.querySelector(".thumbnail-fallback")).not.toBeNull();

    const report = await axe.run(document.body, {
      rules: { "color-contrast": { enabled: false } },
    });
    expect(report.violations).toEqual([]);
  });
});
