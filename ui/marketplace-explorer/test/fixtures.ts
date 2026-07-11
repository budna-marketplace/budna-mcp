import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";

const IMAGE_ID = "0190cafe-0000-7000-8000-000000000001";

export function listing(
  id: number,
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    allow_pickup: true,
    bid_count: 2,
    buy_now_price: null,
    condition: "very_good",
    current_bid: { amount: "1250.50", currency_code: "NOK" },
    end_time: 1_900_000_000_000,
    free_shipping: false,
    has_bids: true,
    id,
    image_urls: [
      `https://images.budna.se/t/listings/${id}/thumbs/${IMAGE_ID}_768x768.webp`,
    ],
    listing_type: "auction",
    listing_url: `https://budna.se/l/${id}`,
    primary_image_url: `https://images.budna.se/t/listings/${id}/thumbs/${IMAGE_ID}_768x768.webp`,
    seller_id: 900 + id,
    shipping_cost: { amount: "79.00", currency_code: "NOK" },
    starting_price: { amount: "1000.00", currency_code: "NOK" },
    status: "active",
    title: `Camera ${id}`,
    ...overrides,
  };
}

export function detailResult(
  id = 1,
  overrides: Record<string, unknown> = {},
): CallToolResult {
  return result({
    ...listing(id),
    buyer_protection_config: {
      cap: { amount: "1000.00", currency_code: "NOK" },
      enabled: true,
      flat_fee: { amount: "10.00", currency_code: "NOK" },
      rate_percent: "4.5",
    },
    description: "A carefully used camera.",
    location: { city: "Oslo", country: "NO", region: "Oslo" },
    seller_name: "Synthetic Seller",
    ...overrides,
  });
}

export function searchResult(
  listings: Record<string, unknown>[] = [listing(1), listing(2)],
  page = 1,
  totalPages = 1,
): CallToolResult {
  return result({
    hits: listings,
    page,
    per_page: 10,
    search_time_ms: 4,
    total: listings.length,
    total_pages: totalPages,
  });
}

export function collectionResult(
  listings: Record<string, unknown>[] = [listing(1)],
  page = 1,
  totalPages = 1,
): CallToolResult {
  return result({
    listings,
    pagination: {
      limit: 10,
      page,
      total: listings.length,
      total_pages: totalPages,
    },
  });
}

export function result(
  structuredContent: Record<string, unknown>,
): CallToolResult {
  return {
    content: [{ text: JSON.stringify(structuredContent), type: "text" }],
    structuredContent,
  };
}
