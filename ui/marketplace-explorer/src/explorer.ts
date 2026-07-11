import type {
  McpUiDisplayMode,
  McpUiHostCapabilities,
  McpUiHostContext,
} from "@modelcontextprotocol/ext-apps";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import {
  supportedLocale,
  translate,
  type SupportedLocale,
  type TranslationKey,
} from "./i18n";
import {
  attributedListingUrl,
  comparisonPayload,
  displayedPrice,
  MAX_SELECTED_LISTINGS,
  MAX_VISIBLE_LISTINGS,
  mergeListings,
  normalizeToolResult,
  type CollectionView,
  type ExplorerView,
  type Listing,
  type Money,
  type ToolSource,
} from "./model";
import {
  PRODUCTION_PUBLIC_ORIGINS,
  type PublicOrigins,
} from "./runtime-config";

export interface HostBridge {
  callTool(
    name: string,
    arguments_: Record<string, unknown>,
  ): Promise<CallToolResult>;
  capabilities(): McpUiHostCapabilities | undefined;
  context(): McpUiHostContext | undefined;
  openLink(url: string): Promise<{ isError?: boolean }>;
  requestDisplayMode(mode: McpUiDisplayMode): Promise<McpUiDisplayMode>;
  sendMessage(text: string): Promise<{ isError?: boolean }>;
  updateModelContext(structuredContent: Record<string, unknown>): Promise<void>;
}

interface StatusMessage {
  kind: "error" | "success" | "warning";
  key: TranslationKey;
}

export class MarketplaceExplorer {
  readonly #root: HTMLElement;
  readonly #bridge: HostBridge;
  readonly #origins: Readonly<PublicOrigins>;
  #connected = false;
  #context: McpUiHostContext = {};
  #history: ExplorerView[] = [];
  #initialArguments: Record<string, unknown> = {};
  #initialToolName = "";
  #loading = true;
  #locale: SupportedLocale = "en";
  #pendingResult?: CallToolResult;
  #selected = new Map<number, Listing>();
  #status?: StatusMessage;
  #timeZone?: string;
  #view?: ExplorerView;

  constructor(
    root: HTMLElement,
    bridge: HostBridge,
    origins: Readonly<PublicOrigins> = PRODUCTION_PUBLIC_ORIGINS,
  ) {
    this.#root = root;
    this.#bridge = bridge;
    this.#origins = { ...origins };
    this.render();
  }

  receiveToolInput(arguments_: Record<string, unknown> | undefined): void {
    this.#initialArguments = arguments_ ? { ...arguments_ } : {};
  }

  receiveToolResult(result: CallToolResult): void {
    if (!this.#connected) {
      this.#pendingResult = result;
      return;
    }
    const source = this.#initialSource();
    this.#applyResult(result, source, "replace");
  }

  receiveCancellation(): void {
    this.#loading = false;
    this.#status = { kind: "warning", key: "cancelled" };
    this.render();
  }

  connected(): void {
    this.#connected = true;
    const context = this.#bridge.context();
    if (context) this.hostContextChanged(context);
    this.#initialToolName =
      context?.toolInfo?.tool.name ?? this.#initialToolName;
    if (this.#pendingResult) {
      const pending = this.#pendingResult;
      this.#pendingResult = undefined;
      this.#applyResult(pending, this.#initialSource(), "replace");
    } else {
      this.render();
    }
  }

  connectionFailed(): void {
    this.#loading = false;
    this.#status = { kind: "error", key: "connectionError" };
    this.render();
  }

  hostContextChanged(context: McpUiHostContext): void {
    this.#context = { ...this.#context, ...context };
    this.#locale = supportedLocale(this.#context.locale);
    this.#timeZone = validTimeZone(this.#context.timeZone);
    document.documentElement.lang = this.#locale === "no" ? "nb" : this.#locale;
    if (this.#context.theme) applyTheme(this.#context.theme);
    if (this.#context.styles?.variables) {
      applyStyleVariables(this.#context.styles.variables);
    }
    this.#applySafeArea(this.#context.safeAreaInsets);
    this.render();
  }

  render(): void {
    const main = node("main", "explorer");
    main.setAttribute("aria-labelledby", "explorer-title");
    main.append(this.#renderHeader());

    const status = node("div", "status-region");
    status.setAttribute("aria-live", "polite");
    status.setAttribute("aria-atomic", "true");
    if (this.#status) {
      status.classList.add(`status-${this.#status.kind}`);
      status.textContent = this.#t(this.#status.key);
    }
    main.append(status);

    if (this.#loading && !this.#view) {
      main.append(this.#renderLoading());
    } else if (this.#view?.kind === "collection") {
      main.append(this.#renderCollection(this.#view));
    } else if (this.#view?.kind === "detail") {
      main.append(this.#renderDetail(this.#view.listing));
    } else if (!this.#loading) {
      main.append(this.#renderEmpty(this.#status?.key ?? "malformed"));
    }

    if (this.#loading && this.#view) main.append(this.#renderProgressOverlay());
    if (this.#selected.size > 0) main.append(this.#renderCompareBar());
    this.#root.replaceChildren(main);
  }

  #initialSource(): ToolSource {
    const inferred = this.#initialToolName || "search_listings";
    return { arguments: { ...this.#initialArguments }, name: inferred };
  }

  #applyResult(
    result: CallToolResult,
    source: ToolSource,
    mode: "append" | "push" | "replace",
  ): void {
    this.#loading = false;
    const normalized = normalizeToolResult(result, source, this.#origins);
    if (!normalized.ok) {
      this.#status = {
        kind: "error",
        key: normalized.reason === "error" ? "toolError" : "malformed",
      };
      this.render();
      return;
    }

    this.#status = undefined;
    this.#refreshSelectedListings(normalized.view);
    if (
      mode === "append" &&
      this.#view?.kind === "collection" &&
      normalized.view.kind === "collection"
    ) {
      this.#view = {
        ...normalized.view,
        listings: mergeListings(this.#view.listings, normalized.view.listings),
        source: this.#view.source,
      };
    } else {
      if (mode === "push" && this.#view) {
        this.#history = [...this.#history.slice(-9), this.#view];
      }
      this.#view = normalized.view;
    }
    this.render();
  }

  async #callTool(
    source: ToolSource,
    mode: "append" | "push" | "replace",
  ): Promise<void> {
    if (!this.#supportsServerTools()) {
      this.#status = { kind: "warning", key: "actionFailed" };
      this.render();
      return;
    }
    this.#loading = true;
    this.#status = undefined;
    this.render();
    try {
      const result = await this.#bridge.callTool(source.name, source.arguments);
      this.#applyResult(result, source, mode);
    } catch {
      this.#loading = false;
      this.#status = { kind: "error", key: "toolError" };
      this.render();
    }
  }

  #renderHeader(): HTMLElement {
    const header = node("header", "topbar");
    const identity = node("div", "identity");
    const mark = node("span", "brand-mark", "B");
    mark.setAttribute("aria-hidden", "true");
    const titles = node("div", "title-group");
    const eyebrow = node("p", "eyebrow", "Budna");
    const title = node("h1", "view-title", this.#viewTitle());
    title.id = "explorer-title";
    titles.append(eyebrow, title);
    identity.append(mark, titles);

    const actions = node("div", "topbar-actions");
    if (this.#history.length > 0) {
      actions.append(
        this.#button(this.#t("back"), "secondary compact", () =>
          this.#goBack(),
        ),
      );
    }
    if (this.#view) {
      const refresh = this.#button(
        this.#t("refresh"),
        "secondary compact",
        () => {
          if (this.#view) void this.#callTool(this.#view.source, "replace");
        },
      );
      refresh.disabled = this.#loading || !this.#supportsServerTools();
      actions.append(refresh);
    }
    const displayButton = this.#renderDisplayModeButton();
    if (displayButton) actions.append(displayButton);
    header.append(identity, actions);
    return header;
  }

  #renderDisplayModeButton(): HTMLButtonElement | undefined {
    const modes = this.#context.availableDisplayModes ?? [];
    const current = this.#context.displayMode ?? "inline";
    const target: McpUiDisplayMode =
      current === "fullscreen" ? "inline" : "fullscreen";
    if (!modes.includes(target)) return undefined;
    return this.#button(
      this.#t(target === "fullscreen" ? "fullscreen" : "inline"),
      "secondary compact",
      async () => {
        try {
          const actual = await this.#bridge.requestDisplayMode(target);
          this.#context = { ...this.#context, displayMode: actual };
          this.render();
        } catch {
          this.#status = { kind: "warning", key: "actionFailed" };
          this.render();
        }
      },
    );
  }

  #renderLoading(): HTMLElement {
    const panel = node("section", "state-panel");
    panel.setAttribute("role", "status");
    panel.append(
      node("span", "spinner"),
      node("p", "state-title", this.#t("loading")),
    );
    return panel;
  }

  #renderProgressOverlay(): HTMLElement {
    const progress = node("div", "progress-line");
    progress.setAttribute("role", "progressbar");
    progress.setAttribute("aria-label", this.#t("loading"));
    return progress;
  }

  #renderEmpty(key: TranslationKey): HTMLElement {
    const panel = node("section", "state-panel");
    panel.append(
      node("span", "empty-mark", "B"),
      node("p", "state-title", this.#t(key)),
    );
    return panel;
  }

  #renderCollection(view: CollectionView): HTMLElement {
    const section = node("section", "collection");
    const meta = node(
      "p",
      "result-meta",
      `${view.total.toLocaleString(this.#locale)} ${this.#t("results")}`,
    );
    section.append(meta);
    if (view.listings.length === 0) {
      section.append(this.#renderEmpty("empty"));
      return section;
    }

    const list = node("ul", "listing-grid");
    list.setAttribute("aria-label", this.#viewTitle());
    for (const listing of view.listings) {
      const item = node("li", "listing-grid-item");
      item.append(this.#renderCard(listing));
      list.append(item);
    }
    section.append(list);

    if (
      view.page < view.totalPages &&
      view.listings.length < MAX_VISIBLE_LISTINGS
    ) {
      const more = this.#button(
        this.#t("loadMore"),
        "primary load-more",
        () => {
          void this.#loadMore(view);
        },
      );
      more.disabled = this.#loading || !this.#supportsServerTools();
      section.append(more);
    }
    return section;
  }

  #renderCard(listing: Listing): HTMLElement {
    const article = node("article", "listing-card");
    article.append(this.#renderImage(listing, "card-image"));

    const body = node("div", "card-body");
    const title = node("h2", "card-title");
    const detailButton = this.#button(
      listing.title ?? this.#t("untitled"),
      "title-button",
      () => void this.#openDetail(listing),
    );
    detailButton.setAttribute(
      "aria-label",
      `${this.#t("showDetails")}: ${detailButton.textContent}`,
    );
    detailButton.disabled = !this.#supportsServerTools();
    title.append(detailButton);

    const price = displayedPrice(listing);
    const priceGroup = node("div", "price-group");
    priceGroup.append(
      node("span", "price-label", this.#priceLabel(price.kind)),
      node("strong", "price", this.#money(price.money)),
    );

    const badges = node("div", "badges");
    badges.append(
      this.#badge(humanize(listing.status)),
      this.#badge(humanize(listing.condition)),
      this.#badge(humanize(listing.listingType)),
    );

    const facts = node("div", "card-facts");
    const shipping = this.#shippingText(listing);
    if (shipping) facts.append(node("span", "fact", shipping));
    if (listing.allowPickup)
      facts.append(node("span", "fact", this.#t("pickup")));
    if (listing.bidCount !== undefined) {
      const count = listing.bidCount;
      facts.append(
        node(
          "span",
          "fact",
          `${count} ${this.#t(count === 1 ? "bid" : "bids")}`,
        ),
      );
    } else if (listing.listingType.includes("auction")) {
      facts.append(
        node("span", "fact", this.#t(listing.hasBids ? "hasBids" : "noBids")),
      );
    }
    facts.append(
      node("span", "fact", `${this.#t("ends")} ${this.#date(listing.endTime)}`),
    );

    const controls = node("div", "card-controls");
    const selectLabel = node("label", "select-label");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = this.#selected.has(listing.id);
    checkbox.setAttribute(
      "aria-label",
      `${this.#t("selectCompare")}: ${listing.title ?? listing.id}`,
    );
    checkbox.addEventListener("change", () => this.#toggleSelection(listing));
    selectLabel.append(
      checkbox,
      document.createTextNode(this.#t("selectCompare")),
    );
    controls.append(selectLabel);
    const open = this.#button(this.#t("viewOnBudna"), "text-button", () => {
      void this.#openListing(listing);
    });
    open.disabled = !this.#supportsOpenLinks();
    controls.append(open);

    body.append(title, priceGroup, badges, facts, controls);
    article.append(body);
    return article;
  }

  #renderDetail(listing: Listing): HTMLElement {
    const article = node("article", "detail");
    const gallery = this.#renderGallery(listing);
    const content = node("div", "detail-content");

    const title = node(
      "h2",
      "detail-title",
      listing.title ?? this.#t("untitled"),
    );
    const price = displayedPrice(listing);
    const priceGroup = node("div", "detail-price");
    priceGroup.append(
      node("span", "price-label", this.#priceLabel(price.kind)),
      node("strong", "price large", this.#money(price.money)),
    );
    const badges = node("div", "badges");
    badges.append(
      this.#badge(humanize(listing.status)),
      this.#badge(humanize(listing.condition)),
      this.#badge(humanize(listing.listingType)),
    );

    const facts = node("dl", "detail-facts");
    this.#definition(facts, this.#t("status"), humanize(listing.status));
    this.#definition(facts, this.#t("condition"), humanize(listing.condition));
    this.#definition(
      facts,
      this.#t("listingType"),
      humanize(listing.listingType),
    );
    const shipping = this.#shippingText(listing);
    if (shipping) this.#definition(facts, this.#t("shipping"), shipping);
    if (listing.allowPickup)
      this.#definition(facts, this.#t("pickup"), this.#t("available"));
    this.#definition(facts, this.#t("ends"), this.#date(listing.endTime));
    if (listing.location) {
      this.#definition(
        facts,
        this.#t("location"),
        [
          listing.location.city,
          listing.location.region,
          listing.location.country,
        ]
          .filter(Boolean)
          .join(", "),
      );
    }

    content.append(title, priceGroup, badges, facts);
    if (listing.description) {
      const section = node("section", "detail-section");
      section.append(
        node("h3", "section-title", this.#t("description")),
        node("p", "description", listing.description),
      );
      content.append(section);
    }
    if (listing.sellerName || listing.sellerUsername) {
      const seller = node("section", "detail-section");
      const value = listing.sellerName ?? listing.sellerUsername ?? "";
      seller.append(
        node("h3", "section-title", this.#t("seller")),
        node("p", "seller", value),
      );
      content.append(seller);
    }
    if (listing.buyerProtection?.enabled) {
      const protection = node("section", "protection");
      protection.append(
        node("strong", "protection-title", this.#t("buyerProtection")),
        node("span", "protection-mark", "✓"),
      );
      content.append(protection);
    }

    const actions = node("div", "detail-actions");
    const external = this.#button(this.#t("viewOnBudna"), "primary", () => {
      void this.#openListing(listing);
    });
    external.disabled = !this.#supportsOpenLinks();
    actions.append(external);
    const related = this.#button(this.#t("related"), "secondary", () => {
      void this.#callTool(
        {
          arguments: { limit: 10, listing_id: listing.id, page: 1 },
          name: "get_listing_related",
        },
        "push",
      );
    });
    related.disabled = !this.#supportsServerTools();
    actions.append(related);
    const seller = this.#button(this.#t("sellerItems"), "secondary", () => {
      void this.#callTool(
        {
          arguments: { limit: 10, page: 1, seller_id: listing.sellerId },
          name: "get_seller_listings",
        },
        "push",
      );
    });
    seller.disabled = !this.#supportsServerTools();
    actions.append(seller);

    content.append(actions);
    article.append(gallery, content);
    return article;
  }

  #renderImage(listing: Listing, className: string): HTMLElement {
    const frame = node("div", `image-frame ${className}`);
    const fallback = node(
      "span",
      "image-fallback",
      this.#t("imageUnavailable"),
    );
    if (!listing.primaryImageUrl) {
      frame.append(fallback);
      return frame;
    }
    const image = document.createElement("img");
    image.src = listing.primaryImageUrl;
    image.alt = listing.title ?? this.#t("untitled");
    image.loading = "lazy";
    image.decoding = "async";
    image.referrerPolicy = "no-referrer";
    image.addEventListener("error", () => frame.replaceChildren(fallback), {
      once: true,
    });
    frame.append(image);
    return frame;
  }

  #renderGallery(listing: Listing): HTMLElement {
    const gallery = node("section", "gallery");
    gallery.setAttribute("aria-label", this.#t("listingDetails"));
    const images =
      listing.imageUrls.length > 0
        ? listing.imageUrls
        : [listing.primaryImageUrl].filter(isString);
    if (images.length === 0) {
      gallery.append(this.#renderImage(listing, "detail-image"));
      return gallery;
    }
    const mainFrame = node("div", "image-frame detail-image");
    const showMainImage = (url: string, index: number) => {
      const mainImage = document.createElement("img");
      mainImage.src = url;
      mainImage.alt = `${listing.title ?? this.#t("untitled")} ${index + 1}`;
      mainImage.decoding = "async";
      mainImage.referrerPolicy = "no-referrer";
      mainImage.addEventListener(
        "error",
        () =>
          mainFrame.replaceChildren(
            node("span", "image-fallback", this.#t("imageUnavailable")),
          ),
        { once: true },
      );
      mainFrame.replaceChildren(mainImage);
    };
    showMainImage(images[0] ?? "", 0);
    gallery.append(mainFrame);
    if (images.length > 1) {
      const strip = node("div", "thumbnail-strip");
      images.forEach((url, index) => {
        const button = this.#button(
          `${this.#t("listingDetails")} ${index + 1}`,
          "thumbnail-button",
          () => showMainImage(url, index),
        );
        button.setAttribute(
          "aria-label",
          `${this.#t("listingDetails")} ${index + 1}`,
        );
        button.textContent = "";
        const image = document.createElement("img");
        image.src = url;
        image.alt = "";
        image.loading = "lazy";
        image.referrerPolicy = "no-referrer";
        image.addEventListener(
          "error",
          () => {
            button.disabled = true;
            button.setAttribute(
              "aria-label",
              `${this.#t("imageUnavailable")} ${index + 1}`,
            );
            button.replaceChildren(node("span", "thumbnail-fallback", "×"));
          },
          { once: true },
        );
        button.append(image);
        strip.append(button);
      });
      gallery.append(strip);
    }
    return gallery;
  }

  #renderCompareBar(): HTMLElement {
    const bar = node("aside", "compare-bar");
    bar.setAttribute("aria-label", this.#t("selectCompare"));
    const count = node(
      "strong",
      "compare-count",
      this.#t("selectedCount", { count: this.#selected.size }),
    );
    const compare = this.#button(this.#t("askAiCompare"), "primary", () => {
      void this.#compareSelected();
    });
    compare.disabled = this.#selected.size < 2 || this.#loading;
    bar.append(count, compare);
    return bar;
  }

  async #loadMore(view: CollectionView): Promise<void> {
    const remaining = MAX_VISIBLE_LISTINGS - view.listings.length;
    if (remaining <= 0 || view.page >= view.totalPages) return;
    const requestedLimit = positiveInteger(view.source.arguments.limit) ?? 10;
    const source: ToolSource = {
      arguments: {
        ...view.source.arguments,
        limit: Math.min(requestedLimit, remaining),
        page: view.page + 1,
      },
      name: view.source.name,
    };
    await this.#callTool(source, "append");
  }

  async #openDetail(listing: Listing): Promise<void> {
    await this.#callTool(
      { arguments: { listing_id: listing.id }, name: "get_listing" },
      "push",
    );
  }

  async #openListing(listing: Listing): Promise<void> {
    const url = attributedListingUrl(listing.listingUrl, this.#origins);
    if (!url || !this.#supportsOpenLinks()) {
      this.#status = { kind: "warning", key: "linkUnavailable" };
      this.render();
      return;
    }
    try {
      const result = await this.#bridge.openLink(url);
      if (result.isError) this.#status = { kind: "warning", key: "linkDenied" };
    } catch {
      this.#status = { kind: "warning", key: "linkDenied" };
    }
    this.render();
  }

  #toggleSelection(listing: Listing): void {
    if (this.#selected.delete(listing.id)) {
      this.#status = undefined;
      this.render();
      return;
    }
    if (this.#selected.size >= MAX_SELECTED_LISTINGS) {
      this.#status = { kind: "warning", key: "compareMax" };
      this.render();
      return;
    }
    this.#selected.set(listing.id, listing);
    this.#status = undefined;
    this.render();
  }

  async #compareSelected(): Promise<void> {
    const listings = [...this.#selected.values()];
    const payload = comparisonPayload(listings);
    if (!payload) {
      this.#status = {
        kind: "warning",
        key: listings.length < 2 ? "compareMin" : "compareMax",
      };
      this.render();
      return;
    }
    if (!this.#supportsComparison()) {
      this.#status = { kind: "warning", key: "compareUnavailable" };
      this.render();
      return;
    }
    try {
      await this.#bridge.updateModelContext(
        payload as unknown as Record<string, unknown>,
      );
      const result = await this.#bridge.sendMessage(
        "Compare the selected Budna listings and explain the important trade-offs.",
      );
      this.#status = result.isError
        ? { kind: "warning", key: "compareUnavailable" }
        : { kind: "success", key: "compareSent" };
    } catch {
      this.#status = { kind: "warning", key: "compareUnavailable" };
    }
    this.render();
  }

  #goBack(): void {
    const previous = this.#history.at(-1);
    if (!previous) return;
    this.#history = this.#history.slice(0, -1);
    this.#view = previous;
    this.#status = undefined;
    this.render();
  }

  #refreshSelectedListings(view: ExplorerView): void {
    const listings =
      view.kind === "collection" ? view.listings : [view.listing];
    for (const listing of listings) {
      if (this.#selected.has(listing.id))
        this.#selected.set(listing.id, listing);
    }
  }

  #applySafeArea(insets: McpUiHostContext["safeAreaInsets"]): void {
    const bounded = (value: number | undefined) =>
      typeof value === "number" && Number.isFinite(value)
        ? `${Math.min(100, Math.max(0, value))}px`
        : "0px";
    this.#root.style.setProperty("--safe-top", bounded(insets?.top));
    this.#root.style.setProperty("--safe-right", bounded(insets?.right));
    this.#root.style.setProperty("--safe-bottom", bounded(insets?.bottom));
    this.#root.style.setProperty("--safe-left", bounded(insets?.left));
  }

  #supportsServerTools(): boolean {
    return this.#bridge.capabilities()?.serverTools !== undefined;
  }

  #supportsOpenLinks(): boolean {
    return this.#bridge.capabilities()?.openLinks !== undefined;
  }

  #supportsComparison(): boolean {
    const capabilities = this.#bridge.capabilities();
    return (
      capabilities?.updateModelContext?.structuredContent !== undefined &&
      capabilities.message?.text !== undefined
    );
  }

  #viewTitle(): string {
    if (!this.#view) return this.#t("appName");
    if (this.#view.kind === "detail") return this.#t("listingDetails");
    if (this.#view.title === "seller") return this.#t("sellerListings");
    if (this.#view.title === "related") return this.#t("relatedListings");
    return this.#t("searchResults");
  }

  #priceLabel(kind: ReturnType<typeof displayedPrice>["kind"]): string {
    if (kind === "current_bid") return this.#t("currentBid");
    if (kind === "buy_now") return this.#t("buyNow");
    return this.#t("startingPrice");
  }

  #money(money: Money): string {
    return `${money.amount} ${money.currency_code}`;
  }

  #shippingText(listing: Listing): string | undefined {
    if (listing.freeShipping) return this.#t("freeShipping");
    if (listing.shippingCost)
      return `${this.#t("shipping")} ${this.#money(listing.shippingCost)}`;
    return undefined;
  }

  #date(epoch: number): string {
    try {
      return new Intl.DateTimeFormat(this.#locale, {
        dateStyle: "medium",
        timeStyle: "short",
        timeZone: this.#timeZone,
      }).format(new Date(epoch));
    } catch {
      return new Date(epoch).toISOString();
    }
  }

  #definition(list: HTMLDListElement, term: string, value: string): void {
    const group = node("div", "definition");
    group.append(
      node("dt", "definition-term", term),
      node("dd", "definition-value", value),
    );
    list.append(group);
  }

  #badge(text: string): HTMLElement {
    return node("span", "badge", text);
  }

  #button(
    text: string,
    className: string,
    listener: () => void | Promise<void>,
  ): HTMLButtonElement {
    const button = node(
      "button",
      `button ${className}`,
      text,
    ) as HTMLButtonElement;
    button.type = "button";
    button.addEventListener("click", () => void listener());
    return button;
  }

  #t(
    key: TranslationKey,
    values?: Readonly<Record<string, string | number>>,
  ): string {
    return translate(this.#locale, key, values);
  }
}

function node<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  element.className = className;
  if (text !== undefined) element.textContent = text;
  return element;
}

function humanize(value: string): string {
  const normalized = value.replace(/[_-]+/gu, " ").trim();
  return normalized.length > 0
    ? normalized[0]?.toUpperCase() + normalized.slice(1)
    : value;
}

function positiveInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0
    ? value
    : undefined;
}

function validTimeZone(value: string | undefined): string | undefined {
  if (!value) return undefined;
  try {
    new Intl.DateTimeFormat("en", { timeZone: value }).format();
    return value;
  } catch {
    return undefined;
  }
}

function isString(value: string | undefined): value is string {
  return value !== undefined;
}

function applyTheme(theme: "light" | "dark"): void {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}

function applyStyleVariables(
  variables: Record<string, string | undefined>,
): void {
  for (const [name, value] of Object.entries(variables)) {
    if (
      /^--(?:border|color|font|shadow)-[a-z0-9-]+$/u.test(name) &&
      value !== undefined
    ) {
      document.documentElement.style.setProperty(name, value);
    }
  }
}
