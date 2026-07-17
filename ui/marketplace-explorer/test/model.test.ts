import { describe, expect, it } from "vitest";
import { supportedLocale, translate } from "@budna-ui/i18n";
import {
  attributedListingUrl,
  comparisonPayload,
  dedupeAndCap,
  displayedPrice,
  MAX_VISIBLE_LISTINGS,
  mergeListings,
  normalizeListingAttributes,
  normalizeListingBidSummary,
  normalizeListingRatingSummary,
  normalizeSellerProfile,
  normalizeToolResult,
  type Listing,
} from "@budna-ui/model";
import {
  collectionResult,
  detailResult,
  listing,
  searchResult,
} from "./fixtures";

const ALTERNATE_PUBLIC_ORIGINS = {
  imageOrigin: "https://media.example.com",
  listingOrigin: "https://market.example.com",
} as const;

describe("marketplace result normalization", () => {
  it("normalizes search cards without changing exact money strings", () => {
    const normalized = normalizeToolResult(searchResult(), {
      arguments: { query: "camera" },
      name: "search_listings",
    });

    expect(normalized.ok).toBe(true);
    if (!normalized.ok || normalized.view.kind !== "collection") return;
    expect(normalized.view.listings[0]?.currentBid).toEqual({
      amount: "1250.50",
      currency_code: "NOK",
    });
    expect(normalized.view.listings[0]?.primaryImageUrl).toContain(
      "0190cafe-0000-7000-8000-000000000001",
    );
  });

  it("normalizes collection pagination and its source", () => {
    const normalized = normalizeToolResult(
      collectionResult([listing(7)], 2, 4),
      {
        arguments: { listing_id: 3, page: 2 },
        name: "get_listing_related",
      },
    );

    expect(normalized).toMatchObject({
      ok: true,
      view: { kind: "collection", page: 2, title: "related", totalPages: 4 },
    });
  });

  it("recognizes seller collection results", () => {
    const normalized = normalizeToolResult(collectionResult([listing(8)]), {
      arguments: { seller_id: 908 },
      name: "get_seller_listings",
    });
    expect(normalized).toMatchObject({
      ok: true,
      view: { kind: "collection", title: "seller" },
    });
  });

  it("selects current-bid, buy-now, and starting prices as exact strings", () => {
    const source = { arguments: {}, name: "search_listings" };
    const currentBid = normalizeToolResult(searchResult([listing(1)]), source);
    const buyNow = normalizeToolResult(
      searchResult([
        listing(2, {
          buy_now_price: { amount: "1999.90", currency_code: "NOK" },
          current_bid: null,
          listing_type: "fixed_price",
        }),
      ]),
      source,
    );
    const starting = normalizeToolResult(
      searchResult([listing(3, { buy_now_price: null, current_bid: null })]),
      source,
    );
    if (
      !currentBid.ok ||
      currentBid.view.kind !== "collection" ||
      !buyNow.ok ||
      buyNow.view.kind !== "collection" ||
      !starting.ok ||
      starting.view.kind !== "collection"
    ) {
      throw new Error("Synthetic listings should normalize");
    }
    const currentListing = currentBid.view.listings[0];
    const buyNowListing = buyNow.view.listings[0];
    const startingListing = starting.view.listings[0];
    if (!currentListing || !buyNowListing || !startingListing) {
      throw new Error("Synthetic listings should be present");
    }
    expect(displayedPrice(currentListing)).toMatchObject({
      kind: "current_bid",
      money: { amount: "1250.50" },
    });
    expect(displayedPrice(buyNowListing)).toMatchObject({
      kind: "buy_now",
      money: { amount: "1999.90" },
    });
    expect(displayedPrice(startingListing)).toMatchObject({
      kind: "starting_price",
      money: { amount: "1000.00" },
    });
  });

  it("normalizes detail-only fields while keeping hostile text as plain data", () => {
    const hostile = '<img src=x onerror="alert(1)">Hej 👋';
    const normalized = normalizeToolResult(
      detailResult(4, { description: hostile, title: hostile }),
      {
        arguments: { listing_id: 4 },
        name: "get_listing",
      },
    );

    expect(normalized.ok).toBe(true);
    if (!normalized.ok || normalized.view.kind !== "detail") return;
    expect(normalized.view.listing.description).toBe(hostile);
    expect(normalized.view.listing.title).toBe(hostile);
  });

  it("rejects unapproved listing URLs and drops unapproved image URLs", () => {
    const badListing = searchResult([
      listing(1, {
        listing_url: "https://example.invalid/l/1",
      }),
    ]);
    expect(
      normalizeToolResult(badListing, {
        arguments: {},
        name: "search_listings",
      }),
    ).toMatchObject({ ok: false, reason: "malformed" });

    const badImage = searchResult([
      listing(1, {
        image_urls: ["https://example.invalid/image.webp"],
        primary_image_url: "https://example.invalid/image.webp",
      }),
    ]);
    const normalized = normalizeToolResult(badImage, {
      arguments: {},
      name: "search_listings",
    });
    expect(normalized.ok).toBe(true);
    if (!normalized.ok || normalized.view.kind !== "collection") return;
    expect(normalized.view.listings[0]?.primaryImageUrl).toBeUndefined();
  });

  it("accepts only URLs from an injected alternate HTTPS origin", () => {
    const imageUrl =
      "https://media.example.com/t/listings/12/thumbs/0190cafe-0000-7000-8000-000000000001_768x768.webp";
    const normalized = normalizeToolResult(
      searchResult([
        listing(12, {
          image_urls: [imageUrl],
          listing_url: "https://market.example.com/l/12",
          primary_image_url: imageUrl,
        }),
      ]),
      { arguments: {}, name: "search_listings" },
      ALTERNATE_PUBLIC_ORIGINS,
    );
    expect(normalized).toMatchObject({
      ok: true,
      view: {
        listings: [
          {
            listingUrl: "https://market.example.com/l/12",
            primaryImageUrl: imageUrl,
          },
        ],
      },
    });
    expect(
      attributedListingUrl(
        "https://market.example.com/l/12",
        ALTERNATE_PUBLIC_ORIGINS,
      ),
    ).toContain("utm_source=budna_mcp");
    expect(
      attributedListingUrl("https://market.example.com/l/12"),
    ).toBeUndefined();
  });

  it("reports malformed and tool-error results", () => {
    expect(
      normalizeToolResult(
        {
          content: [{ text: "bad", type: "text" }],
          structuredContent: { unexpected: true },
        },
        { arguments: {}, name: "search_listings" },
      ),
    ).toMatchObject({ ok: false, reason: "malformed" });
    expect(
      normalizeToolResult(
        { content: [{ text: "failed", type: "text" }], isError: true },
        { arguments: {}, name: "search_listings" },
      ),
    ).toMatchObject({ ok: false, reason: "error" });
  });

  it("normalizes only matching, bounded detail-research projections", () => {
    expect(
      normalizeListingAttributes(
        {
          attributes: [
            {
              display_value: "Black",
              label: "Colour",
              listing_id: 5,
            },
          ],
          listing_id: 5,
          truncated: false,
        },
        5,
      ),
    ).toEqual({
      attributes: [{ displayValue: "Black", label: "Colour" }],
      truncated: false,
    });
    expect(
      normalizeListingBidSummary(
        {
          bid_count: 0,
          current_bid: null,
          listing_id: 5,
          reserve_price_met: false,
        },
        5,
      ),
    ).toEqual({ bidCount: 0, currentBid: undefined, reservePriceMet: false });
    expect(
      normalizeListingRatingSummary(
        {
          average_rating: 4.5,
          listing_id: 5,
          positive_percentage: 92.5,
          total_ratings: 12,
        },
        5,
      ),
    ).toEqual({
      averageRating: 4.5,
      positivePercentage: 92.5,
      totalRatings: 12,
    });
    expect(
      normalizeSellerProfile(
        {
          categories: ["Cameras"],
          display_name: "Seller",
          identity_verified: true,
          is_company: false,
          rating: "4.9",
          seller_id: 9,
          sold_items_count: 3,
          total_ratings: 7,
        },
        9,
      ),
    ).toMatchObject({
      categories: ["Cameras"],
      displayName: "Seller",
      rating: "4.9",
      soldItemsCount: 3,
      totalRatings: 7,
    });
    expect(
      normalizeSellerProfile(
        {
          display_name: "Wrong seller",
          identity_verified: true,
          is_company: false,
          rating: "4.9",
          seller_id: 10,
          sold_items_count: 3,
          total_ratings: 7,
        },
        9,
      ),
    ).toBeUndefined();
  });
});

describe("bounded interaction data", () => {
  const normalizedListings = (): Listing[] => {
    const normalized = normalizeToolResult(searchResult(), {
      arguments: {},
      name: "search_listings",
    });
    if (!normalized.ok || normalized.view.kind !== "collection") return [];
    return normalized.view.listings;
  };

  it("deduplicates listings and enforces the 50-listing ceiling", () => {
    const base = normalizedListings()[0];
    expect(base).toBeDefined();
    if (!base) return;
    const many = Array.from({ length: 60 }, (_, index) => ({
      ...base,
      id: index + 1,
    }));
    expect(dedupeAndCap([many[0] as Listing, ...many])).toHaveLength(
      MAX_VISIBLE_LISTINGS,
    );
    expect(mergeListings(many.slice(0, 30), many.slice(20))).toHaveLength(
      MAX_VISIBLE_LISTINGS,
    );
  });

  it("creates a bounded comparison payload without seller or description fields", () => {
    const payload = comparisonPayload(normalizedListings());
    expect(payload).toMatchObject({
      type: "budna_listing_comparison",
      version: "1",
    });
    expect(payload?.listings[0]?.price.amount).toBe("1250.50");
    expect(payload?.listings[0]).not.toHaveProperty("sellerId");
    expect(payload?.listings[0]).not.toHaveProperty("description");
    expect(comparisonPayload(normalizedListings().slice(0, 1))).toBeUndefined();

    const withoutShippingCost = normalizedListings().map((value) => ({
      ...value,
      shippingCost: undefined,
    }));
    expect(
      comparisonPayload(withoutShippingCost)?.listings[0]?.shipping,
    ).not.toHaveProperty("cost");
  });

  it("adds only the approved anonymous attribution parameters", () => {
    const value = attributedListingUrl("https://budna.se/l/42");
    const url = new URL(value ?? "https://budna.se");
    expect(url.origin + url.pathname).toBe("https://budna.se/l/42");
    expect(Object.fromEntries(url.searchParams)).toEqual({
      utm_campaign: "interactive_cards",
      utm_medium: "ai_assistant",
      utm_source: "budna_mcp",
    });
    expect(
      attributedListingUrl("https://example.invalid/l/42"),
    ).toBeUndefined();
  });
});

describe("localization", () => {
  it("selects English, Swedish, and every Norwegian language code", () => {
    expect(supportedLocale("en-US")).toBe("en");
    expect(supportedLocale("sv-SE")).toBe("sv");
    expect(supportedLocale("no-NO")).toBe("no");
    expect(supportedLocale("nb-NO")).toBe("no");
    expect(supportedLocale("nn-NO")).toBe("no");
    expect(supportedLocale("fr-FR")).toBe("en");
    expect(translate("sv", "loadMore")).toBe("Ladda fler");
    expect(translate("no", "refresh")).toBe("Oppdater");
  });
});
