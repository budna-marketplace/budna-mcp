import type {
  McpUiDisplayMode,
  McpUiHostCapabilities,
  McpUiHostContext,
} from "@modelcontextprotocol/ext-apps";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import type { HostBridge } from "./explorer";

const PROTOCOL_VERSION = "2026-01-26";
const REQUEST_TIMEOUT_MS = 30_000;

type JsonObject = Record<string, unknown>;
type RequestId = number | string;

interface PendingRequest {
  reject: (error: Error) => void;
  resolve: (value: unknown) => void;
  timeout: number;
}

export interface BridgeHandlers {
  hostContextChanged: (context: McpUiHostContext) => void;
  toolCancelled: () => void;
  toolInput: (arguments_: Record<string, unknown> | undefined) => void;
  toolResult: (result: CallToolResult) => void;
}

/**
 * Minimal client for the stable MCP Apps postMessage protocol.
 *
 * The official packages remain the authority for every public protocol type.
 * A small transport is used instead of the optional App convenience runtime so
 * the embedded, dependency-free HTML remains within Budna's 300 KiB limit.
 */
export class McpAppBridge implements HostBridge {
  readonly #target: Window;
  readonly #handlers: BridgeHandlers;
  readonly #pending = new Map<number, PendingRequest>();
  #capabilities?: McpUiHostCapabilities;
  #connected = false;
  #context?: McpUiHostContext;
  #nextId = 1;
  #resizeFrame?: number;
  #resizeObserver?: ResizeObserver;

  constructor(target: Window, handlers: BridgeHandlers) {
    this.#target = target;
    this.#handlers = handlers;
  }

  capabilities(): McpUiHostCapabilities | undefined {
    return this.#capabilities;
  }

  context(): McpUiHostContext | undefined {
    return this.#context;
  }

  async connect(): Promise<void> {
    window.addEventListener("message", this.#receiveMessage);
    try {
      const result = await this.#request("ui/initialize", {
        appCapabilities: { availableDisplayModes: ["inline", "fullscreen"] },
        appInfo: { name: "Budna Marketplace Explorer", version: "0.2.0" },
        protocolVersion: PROTOCOL_VERSION,
      });
      const initialized = parseInitializeResult(result);
      this.#capabilities = initialized.hostCapabilities;
      this.#context = initialized.hostContext;
      this.#connected = true;
      this.#notify("ui/notifications/initialized", {});
      this.#setupAutoResize();
    } catch (error) {
      this.close();
      throw error;
    }
  }

  close(): void {
    window.removeEventListener("message", this.#receiveMessage);
    this.#resizeObserver?.disconnect();
    if (this.#resizeFrame !== undefined)
      cancelAnimationFrame(this.#resizeFrame);
    for (const pending of this.#pending.values()) {
      window.clearTimeout(pending.timeout);
      pending.reject(new Error("MCP App connection closed"));
    }
    this.#pending.clear();
    this.#connected = false;
  }

  async callTool(
    name: string,
    arguments_: Record<string, unknown>,
  ): Promise<CallToolResult> {
    const result = await this.#request("tools/call", {
      arguments: arguments_,
      name,
    });
    return parseToolResult(result);
  }

  async openLink(url: string): Promise<{ isError?: boolean }> {
    return parseActionResult(await this.#request("ui/open-link", { url }));
  }

  async requestDisplayMode(mode: McpUiDisplayMode): Promise<McpUiDisplayMode> {
    const result = asObject(
      await this.#request("ui/request-display-mode", { mode }),
    );
    if (!result || !isDisplayMode(result.mode))
      throw new Error("Invalid display mode response");
    return result.mode;
  }

  async sendMessage(text: string): Promise<{ isError?: boolean }> {
    return parseActionResult(
      await this.#request("ui/message", {
        content: [{ text, type: "text" }],
        role: "user",
      }),
    );
  }

  async updateModelContext(
    structuredContent: Record<string, unknown>,
  ): Promise<void> {
    await this.#request("ui/update-model-context", { structuredContent });
  }

  readonly #receiveMessage = (event: MessageEvent<unknown>): void => {
    if (event.source !== this.#target) return;
    const message = asObject(event.data);
    if (!message || message.jsonrpc !== "2.0") return;

    const id =
      typeof message.id === "number" || typeof message.id === "string"
        ? message.id
        : undefined;
    if (id !== undefined && ("result" in message || "error" in message)) {
      if (typeof id === "number") this.#receiveResponse(id, message);
      return;
    }

    if (typeof message.method !== "string") return;
    if (id !== undefined) {
      this.#receiveHostRequest(id, message.method);
      return;
    }
    this.#receiveNotification(message.method, asObject(message.params) ?? {});
  };

  #receiveResponse(id: number, message: JsonObject): void {
    const pending = this.#pending.get(id);
    if (!pending) return;
    this.#pending.delete(id);
    window.clearTimeout(pending.timeout);
    if ("error" in message) {
      pending.reject(new Error("The MCP App host rejected the request"));
    } else {
      pending.resolve(message.result);
    }
  }

  #receiveHostRequest(id: RequestId, method: string): void {
    if (method === "ping") {
      this.#send({ id, jsonrpc: "2.0", result: {} });
      return;
    }
    if (method === "ui/resource-teardown") {
      this.#send({ id, jsonrpc: "2.0", result: {} });
      this.close();
      return;
    }
    this.#send({
      error: { code: -32_601, message: "Method not found" },
      id,
      jsonrpc: "2.0",
    });
  }

  #receiveNotification(method: string, params: JsonObject): void {
    if (method === "ui/notifications/tool-input") {
      this.#handlers.toolInput(asObject(params.arguments));
      return;
    }
    if (method === "ui/notifications/tool-result") {
      try {
        this.#handlers.toolResult(parseToolResult(params));
      } catch {
        this.#handlers.toolResult({
          content: [{ text: "Invalid tool result", type: "text" }],
          isError: true,
        });
      }
      return;
    }
    if (method === "ui/notifications/tool-cancelled") {
      this.#handlers.toolCancelled();
      return;
    }
    if (method === "ui/notifications/host-context-changed") {
      const context = params as McpUiHostContext;
      this.#context = { ...this.#context, ...context };
      this.#handlers.hostContextChanged(context);
    }
  }

  #request(method: string, params: JsonObject): Promise<unknown> {
    const id = this.#nextId++;
    return new Promise((resolve, reject) => {
      const timeout = window.setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error("The MCP App host request timed out"));
      }, REQUEST_TIMEOUT_MS);
      this.#pending.set(id, { reject, resolve, timeout });
      this.#send({ id, jsonrpc: "2.0", method, params });
    });
  }

  #notify(method: string, params: JsonObject): void {
    this.#send({ jsonrpc: "2.0", method, params });
  }

  #send(message: JsonObject): void {
    this.#target.postMessage(message, "*");
  }

  #setupAutoResize(): void {
    if (typeof ResizeObserver === "undefined") return;
    const report = () => {
      if (!this.#connected || this.#resizeFrame !== undefined) return;
      this.#resizeFrame = requestAnimationFrame(() => {
        this.#resizeFrame = undefined;
        const root = document.documentElement;
        const body = document.body;
        const width = Math.ceil(Math.max(root.scrollWidth, body.scrollWidth));
        const height = Math.ceil(
          Math.max(root.scrollHeight, body.scrollHeight),
        );
        this.#notify("ui/notifications/size-changed", { height, width });
      });
    };
    this.#resizeObserver = new ResizeObserver(report);
    this.#resizeObserver.observe(document.documentElement);
    this.#resizeObserver.observe(document.body);
    report();
  }
}

function parseInitializeResult(value: unknown): {
  hostCapabilities: McpUiHostCapabilities;
  hostContext: McpUiHostContext;
} {
  const root = asObject(value);
  const capabilities = asObject(root?.hostCapabilities);
  const context = asObject(root?.hostContext);
  if (
    !root ||
    !capabilities ||
    !context ||
    typeof root.protocolVersion !== "string"
  ) {
    throw new Error("Invalid MCP App initialization response");
  }
  return {
    hostCapabilities: capabilities as McpUiHostCapabilities,
    hostContext: context as McpUiHostContext,
  };
}

function parseToolResult(value: unknown): CallToolResult {
  const root = asObject(value);
  if (!root || !Array.isArray(root.content))
    throw new Error("Invalid MCP tool result");
  if (root.isError !== undefined && typeof root.isError !== "boolean") {
    throw new Error("Invalid MCP tool error flag");
  }
  return root as unknown as CallToolResult;
}

function parseActionResult(value: unknown): { isError?: boolean } {
  const root = asObject(value);
  if (
    !root ||
    (root.isError !== undefined && typeof root.isError !== "boolean")
  ) {
    throw new Error("Invalid MCP App action response");
  }
  return root.isError === true ? { isError: true } : {};
}

function asObject(value: unknown): JsonObject | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as JsonObject)
    : undefined;
}

function isDisplayMode(value: unknown): value is McpUiDisplayMode {
  return value === "inline" || value === "fullscreen" || value === "pip";
}
