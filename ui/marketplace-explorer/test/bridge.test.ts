// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { McpAppBridge, type BridgeHandlers } from "@budna-ui/bridge";
import { searchResult } from "./fixtures";

interface SentMessage {
  id?: number | string;
  jsonrpc: "2.0";
  method?: string;
  params?: Record<string, unknown>;
  result?: Record<string, unknown>;
}

function harness() {
  const sent: SentMessage[] = [];
  const target = {
    postMessage: vi.fn((message: SentMessage) => sent.push(message)),
  } as unknown as Window;
  const handlers: BridgeHandlers = {
    hostContextChanged: vi.fn(),
    toolCancelled: vi.fn(),
    toolInput: vi.fn(),
    toolResult: vi.fn(),
  };
  const bridge = new McpAppBridge(target, handlers);
  const receive = (data: Record<string, unknown>) => {
    window.dispatchEvent(
      new MessageEvent("message", {
        data,
        source: target,
      }),
    );
  };
  return { bridge, handlers, receive, sent, target };
}

async function connect(h: ReturnType<typeof harness>): Promise<void> {
  const pending = h.bridge.connect();
  const initialize = h.sent[0];
  expect(initialize?.method).toBe("ui/initialize");
  expect(initialize?.params).toMatchObject({
    appInfo: { name: "Budna Marketplace Explorer", version: "0.2.0" },
    protocolVersion: "2026-01-26",
  });
  h.receive({
    id: initialize?.id,
    jsonrpc: "2.0",
    result: {
      hostCapabilities: {
        openLinks: {},
        serverTools: {},
      },
      hostContext: {
        locale: "en-US",
        toolInfo: {
          tool: { inputSchema: { type: "object" }, name: "search_listings" },
        },
      },
      hostInfo: { name: "Synthetic Host", version: "1" },
      protocolVersion: "2026-01-26",
    },
  });
  await pending;
  expect(h.sent.at(-1)?.method).toBe("ui/notifications/initialized");
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("McpAppBridge", () => {
  it("performs the stable MCP Apps initialization handshake", async () => {
    const h = harness();
    await connect(h);
    expect(h.bridge.capabilities()).toMatchObject({
      openLinks: {},
      serverTools: {},
    });
    expect(h.bridge.context()).toMatchObject({ locale: "en-US" });
    h.bridge.close();
  });

  it("forwards tool calls and validates their result", async () => {
    const h = harness();
    await connect(h);
    const pending = h.bridge.callTool("get_listing", { listing_id: 42 });
    const call = h.sent.at(-1);
    expect(call).toMatchObject({
      method: "tools/call",
      params: { arguments: { listing_id: 42 }, name: "get_listing" },
    });
    h.receive({ id: call?.id, jsonrpc: "2.0", result: searchResult() });
    await expect(pending).resolves.toMatchObject({
      structuredContent: { page: 1 },
    });
    h.bridge.close();
  });

  it("delivers host tool and context notifications", async () => {
    const h = harness();
    await connect(h);
    h.receive({
      jsonrpc: "2.0",
      method: "ui/notifications/tool-input",
      params: { arguments: { query: "camera" } },
    });
    h.receive({
      jsonrpc: "2.0",
      method: "ui/notifications/tool-result",
      params: searchResult(),
    });
    h.receive({
      jsonrpc: "2.0",
      method: "ui/notifications/host-context-changed",
      params: { locale: "sv-SE", theme: "dark" },
    });
    h.receive({
      jsonrpc: "2.0",
      method: "ui/notifications/tool-cancelled",
      params: {},
    });

    expect(h.handlers.toolInput).toHaveBeenCalledWith({ query: "camera" });
    expect(h.handlers.toolResult).toHaveBeenCalledOnce();
    expect(h.handlers.hostContextChanged).toHaveBeenCalledWith({
      locale: "sv-SE",
      theme: "dark",
    });
    expect(h.handlers.toolCancelled).toHaveBeenCalledOnce();
    h.bridge.close();
  });

  it("uses host-mediated open-link, context, message, and display requests", async () => {
    const h = harness();
    await connect(h);

    const open = h.bridge.openLink(
      "https://budna.se/l/42?utm_source=budna_mcp",
    );
    let request = h.sent.at(-1);
    expect(request?.method).toBe("ui/open-link");
    h.receive({ id: request?.id, jsonrpc: "2.0", result: {} });
    await expect(open).resolves.toEqual({});

    const context = h.bridge.updateModelContext({
      type: "budna_listing_comparison",
    });
    request = h.sent.at(-1);
    expect(request?.method).toBe("ui/update-model-context");
    h.receive({ id: request?.id, jsonrpc: "2.0", result: {} });
    await context;

    const message = h.bridge.sendMessage("Compare these listings");
    request = h.sent.at(-1);
    expect(request?.method).toBe("ui/message");
    h.receive({ id: request?.id, jsonrpc: "2.0", result: {} });
    await expect(message).resolves.toEqual({});

    const display = h.bridge.requestDisplayMode("fullscreen");
    request = h.sent.at(-1);
    expect(request?.method).toBe("ui/request-display-mode");
    h.receive({
      id: request?.id,
      jsonrpc: "2.0",
      result: { mode: "fullscreen" },
    });
    await expect(display).resolves.toBe("fullscreen");
    h.bridge.close();
  });

  it("ignores messages from another source and acknowledges host requests", async () => {
    const h = harness();
    const pending = h.bridge.connect();
    const initialize = h.sent[0];
    window.dispatchEvent(
      new MessageEvent("message", {
        data: { id: initialize?.id, jsonrpc: "2.0", result: {} },
        source: window,
      }),
    );
    expect(h.sent).toHaveLength(1);
    h.receive({
      id: initialize?.id,
      jsonrpc: "2.0",
      result: {
        hostCapabilities: {},
        hostContext: {},
        hostInfo: { name: "Host", version: "1" },
        protocolVersion: "2026-01-26",
      },
    });
    await pending;

    h.receive({
      id: "ping-42",
      jsonrpc: "2.0",
      method: "ping",
      params: {},
    });
    expect(h.sent.at(-1)).toMatchObject({ id: "ping-42", result: {} });

    h.receive({
      id: "teardown-99",
      jsonrpc: "2.0",
      method: "ui/resource-teardown",
      params: {},
    });
    expect(h.sent.at(-1)).toMatchObject({ id: "teardown-99", result: {} });
  });
});
