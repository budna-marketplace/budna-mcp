import { McpAppBridge } from "./bridge";
import { MarketplaceExplorer } from "./explorer";
import { readPublicOrigins } from "./runtime-config";
import "./style.css";

const root = document.getElementById("app");
if (!root) throw new Error("Marketplace Explorer root is missing");

let explorer: MarketplaceExplorer;
const origins = readPublicOrigins();
const bridge = new McpAppBridge(window.parent, {
  hostContextChanged: (context) => explorer.hostContextChanged(context),
  toolCancelled: () => explorer.receiveCancellation(),
  toolInput: (arguments_) => explorer.receiveToolInput(arguments_),
  toolResult: (result) => explorer.receiveToolResult(result),
});
explorer = new MarketplaceExplorer(root, bridge, origins);

void bridge
  .connect()
  .then(() => explorer.connected())
  .catch(() => explorer.connectionFailed());
