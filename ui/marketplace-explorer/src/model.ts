import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import {
  PRODUCTION_PUBLIC_ORIGINS,
  type PublicOrigins,
} from "./runtime-config";

export const MAX_VISIBLE_LISTINGS = 50;
export const MAX_SELECTED_LISTINGS = 4;
export const MIN_SELECTED_LISTINGS = 2;

export interface Money {
  amount: string;
  currency_code: string;
}

export interface PublicLocation {
  city: string;
  region?: string;
  country: string;
}

export interface BuyerProtection {
  enabled: boolean;
  ratePercent?: string;
  flatFee?: Money;
  cap?: Money;
}

export interface ListingAttribute {
  label: string;
  displayValue: string;
}

export interface ListingAttributes {
  attributes: ListingAttribute[];
  truncated: boolean;
}

export interface ListingBidSummary {
  bidCount?: number;
  currentBid?: Money;
  reservePriceMet: boolean;
}

export interface ListingRatingSummary {
  averageRating: number;
  positivePercentage: number;
  totalRatings: number;
}

export interface SellerProfile {
  bio?: string;
  categories: string[];
  city?: string;
  country?: string;
  displayName: string;
  identityVerified: boolean;
  isCompany: boolean;
  rating: string;
  soldItemsCount: number;
  totalRatings: number;
  username?: string;
}

export interface Listing {
  id: number;
  sellerId: number;
  sellerName?: string;
  sellerUsername?: string;
  title?: string;
  description?: string;
  categoryName?: string;
  condition: string;
  listingType: string;
  status: string;
  listingUrl: string;
  primaryImageUrl?: string;
  imageUrls: string[];
  startingPrice: Money;
  currentBid?: Money;
  buyNowPrice?: Money;
  shippingCost?: Money;
  freeShipping: boolean;
  allowPickup: boolean;
  endTime: number;
  bidCount?: number;
  hasBids: boolean;
  location?: PublicLocation;
  buyerProtection?: BuyerProtection;
}

export interface ToolSource {
  name: string;
  arguments: Record<string, unknown>;
}

export interface CollectionView {
  kind: "collection";
  title: "search" | "related" | "seller";
  listings: Listing[];
  page: number;
  total: number;
  totalPages: number;
  source: ToolSource;
}

export interface DetailView {
  kind: "detail";
  listing: Listing;
  source: ToolSource;
}

export type ExplorerView = CollectionView | DetailView;

export type NormalizedResult =
  | { ok: true; view: ExplorerView }
  | { ok: false; reason: "error" | "malformed"; message?: string };

interface JsonObject {
  [key: string]: unknown;
}

export function normalizeToolResult(
  result: CallToolResult,
  source: ToolSource,
  origins: Readonly<PublicOrigins> = PRODUCTION_PUBLIC_ORIGINS,
): NormalizedResult {
  if (result.isError) {
    return { ok: false, reason: "error", message: firstTextContent(result) };
  }

  const root = asObject(result.structuredContent);
  if (!root) return { ok: false, reason: "malformed" };

  if (Array.isArray(root.hits)) {
    return normalizeCollection(root, source, "hits", "search", origins);
  }
  if (Array.isArray(root.listings)) {
    const title = source.name === "get_seller_listings" ? "seller" : "related";
    return normalizeCollection(root, source, "listings", title, origins);
  }
  if (source.name === "get_listing" || looksLikeDetail(root)) {
    const listing = normalizeListing(root, true, origins);
    return listing
      ? { ok: true, view: { kind: "detail", listing, source } }
      : { ok: false, reason: "malformed" };
  }

  return { ok: false, reason: "malformed" };
}

function normalizeCollection(
  root: JsonObject,
  source: ToolSource,
  field: "hits" | "listings",
  title: CollectionView["title"],
  origins: Readonly<PublicOrigins>,
): NormalizedResult {
  const rawListings = root[field];
  if (!Array.isArray(rawListings)) return { ok: false, reason: "malformed" };

  const listings = dedupeAndCap(
    rawListings
      .map((value) => normalizeListing(asObject(value), false, origins))
      .filter((value): value is Listing => value !== undefined),
  );
  if (rawListings.length > 0 && listings.length === 0) {
    return { ok: false, reason: "malformed" };
  }

  const pagination = asObject(root.pagination);
  const page =
    positiveInteger(root.page) ?? positiveInteger(pagination?.page) ?? 1;
  const totalPages =
    positiveInteger(root.total_pages) ??
    positiveInteger(pagination?.total_pages) ??
    page;
  const total =
    nonNegativeInteger(root.total) ??
    nonNegativeInteger(pagination?.total) ??
    listings.length;

  return {
    ok: true,
    view: {
      kind: "collection",
      listings,
      page,
      source,
      title,
      total,
      totalPages: Math.max(page, totalPages),
    },
  };
}

function normalizeListing(
  root: JsonObject | undefined,
  detail: boolean,
  origins: Readonly<PublicOrigins>,
): Listing | undefined {
  if (!root) return undefined;
  const id = positiveInteger(root.id);
  const sellerId = positiveInteger(root.seller_id);
  if (!id || !sellerId) return undefined;

  const listingUrl = cleanListingUrl(root.listing_url, id, origins);
  const startingPrice = normalizeMoney(root.starting_price);
  const condition = boundedString(root.condition, 100);
  const listingType = boundedString(root.listing_type, 100);
  const status = boundedString(root.status, 100);
  const endTime = epochMilliseconds(root.end_time);
  if (
    !listingUrl ||
    !startingPrice ||
    !condition ||
    !listingType ||
    !status ||
    !endTime
  ) {
    return undefined;
  }

  const suppliedImages = Array.isArray(root.image_urls) ? root.image_urls : [];
  const imageUrls = suppliedImages
    .map((value) => cleanImageUrl(value, id, origins))
    .filter((value): value is string => value !== undefined)
    .slice(0, 8);
  const primaryImageUrl =
    cleanImageUrl(root.primary_image_url, id, origins) ?? imageUrls[0];
  if (primaryImageUrl && !imageUrls.includes(primaryImageUrl))
    imageUrls.unshift(primaryImageUrl);

  const bidCount = nonNegativeInteger(root.bid_count);
  return {
    allowPickup: root.allow_pickup === true,
    bidCount,
    buyerProtection: normalizeBuyerProtection(root.buyer_protection_config),
    buyNowPrice: normalizeMoney(root.buy_now_price),
    categoryName: boundedString(root.category_name, 250),
    condition,
    currentBid: normalizeMoney(root.current_bid),
    description: detail ? boundedString(root.description, 20_000) : undefined,
    endTime,
    freeShipping: root.free_shipping === true,
    hasBids: root.has_bids === true || (bidCount !== undefined && bidCount > 0),
    id,
    imageUrls: imageUrls.slice(0, 8),
    listingType,
    listingUrl,
    location: normalizeLocation(root.location),
    primaryImageUrl,
    sellerId,
    sellerName: boundedString(root.seller_name, 250),
    sellerUsername: boundedString(root.seller_username, 250),
    shippingCost: normalizeMoney(root.shipping_cost),
    startingPrice,
    status,
    title: boundedString(root.title, 500),
  };
}

export function dedupeAndCap(listings: readonly Listing[]): Listing[] {
  const seen = new Set<number>();
  const result: Listing[] = [];
  for (const listing of listings) {
    if (seen.has(listing.id)) continue;
    seen.add(listing.id);
    result.push(listing);
    if (result.length === MAX_VISIBLE_LISTINGS) break;
  }
  return result;
}

export function mergeListings(
  current: readonly Listing[],
  incoming: readonly Listing[],
): Listing[] {
  return dedupeAndCap([...current, ...incoming]);
}

export function displayedPrice(listing: Listing): {
  kind: "buy_now" | "current_bid" | "starting_price";
  money: Money;
} {
  if (listing.currentBid)
    return { kind: "current_bid", money: listing.currentBid };
  if (listing.buyNowPrice)
    return { kind: "buy_now", money: listing.buyNowPrice };
  return { kind: "starting_price", money: listing.startingPrice };
}

export function attributedListingUrl(
  listingUrl: string,
  origins: Readonly<PublicOrigins> = PRODUCTION_PUBLIC_ORIGINS,
): string | undefined {
  const clean = cleanListingUrlWithoutKnownId(listingUrl, origins);
  if (!clean) return undefined;
  const url = new URL(clean);
  url.searchParams.set("utm_source", "budna_mcp");
  url.searchParams.set("utm_medium", "ai_assistant");
  url.searchParams.set("utm_campaign", "interactive_cards");
  return url.toString();
}

export interface ComparisonListing {
  id: number;
  title: string;
  listing_url: string;
  price: Money;
  condition: string;
  shipping: {
    free: boolean;
    pickup: boolean;
    cost?: Money;
  };
  status: string;
  end_time: number;
}

export interface ComparisonPayload {
  type: "budna_listing_comparison";
  version: "1";
  listings: ComparisonListing[];
}

export function comparisonPayload(
  listings: readonly Listing[],
): ComparisonPayload | undefined {
  if (
    listings.length < MIN_SELECTED_LISTINGS ||
    listings.length > MAX_SELECTED_LISTINGS
  ) {
    return undefined;
  }
  return {
    listings: listings.map((listing) => ({
      condition: listing.condition.slice(0, 100),
      end_time: listing.endTime,
      id: listing.id,
      listing_url: listing.listingUrl,
      price: displayedPrice(listing).money,
      shipping: {
        free: listing.freeShipping,
        pickup: listing.allowPickup,
        ...(listing.shippingCost ? { cost: listing.shippingCost } : {}),
      },
      status: listing.status.slice(0, 100),
      title: (listing.title ?? "Untitled listing").slice(0, 160),
    })),
    type: "budna_listing_comparison",
    version: "1",
  };
}

export function normalizeListingAttributes(
  value: unknown,
  listingId: number,
): ListingAttributes | undefined {
  const root = asObject(value);
  if (!root || positiveInteger(root.listing_id) !== listingId) return undefined;
  if (!Array.isArray(root.attributes)) return undefined;

  const attributes: ListingAttribute[] = [];
  for (const value of root.attributes.slice(0, 100)) {
    const attribute = asObject(value);
    if (positiveInteger(attribute?.listing_id) !== listingId) continue;
    const label = boundedString(attribute?.label, 256);
    const displayValue = boundedString(attribute?.display_value, 512);
    if (!label || !displayValue) continue;
    attributes.push({ displayValue, label });
  }
  if (root.attributes.length > 0 && attributes.length === 0) return undefined;
  return { attributes, truncated: root.truncated === true };
}

export function normalizeListingBidSummary(
  value: unknown,
  listingId: number,
): ListingBidSummary | undefined {
  const root = asObject(value);
  if (
    !root ||
    positiveInteger(root.listing_id) !== listingId ||
    typeof root.reserve_price_met !== "boolean"
  ) {
    return undefined;
  }
  const bidCount = nonNegativeInteger(root.bid_count);
  if (
    root.bid_count !== null &&
    root.bid_count !== undefined &&
    bidCount === undefined
  )
    return undefined;
  const currentBid = normalizeMoney(root.current_bid);
  if (
    root.current_bid !== null &&
    root.current_bid !== undefined &&
    currentBid === undefined
  )
    return undefined;
  return {
    bidCount,
    currentBid,
    reservePriceMet: root.reserve_price_met,
  };
}

export function normalizeListingRatingSummary(
  value: unknown,
  listingId: number,
): ListingRatingSummary | undefined {
  const root = asObject(value);
  if (!root || positiveInteger(root.listing_id) !== listingId) return undefined;
  const averageRating = finiteNumberInRange(root.average_rating, 0, 5);
  const positivePercentage = finiteNumberInRange(
    root.positive_percentage,
    0,
    100,
  );
  const totalRatings = nonNegativeInteger(root.total_ratings);
  if (
    averageRating === undefined ||
    positivePercentage === undefined ||
    totalRatings === undefined
  ) {
    return undefined;
  }
  return { averageRating, positivePercentage, totalRatings };
}

export function normalizeSellerProfile(
  value: unknown,
  sellerId: number,
): SellerProfile | undefined {
  const root = asObject(value);
  if (!root || positiveInteger(root.seller_id) !== sellerId) return undefined;
  const displayName = boundedString(root.display_name, 256);
  const rating = boundedString(root.rating, 128);
  const totalRatings = nonNegativeInteger(root.total_ratings);
  const soldItemsCount = nonNegativeInteger(root.sold_items_count);
  if (
    !displayName ||
    !rating ||
    totalRatings === undefined ||
    soldItemsCount === undefined ||
    typeof root.identity_verified !== "boolean" ||
    typeof root.is_company !== "boolean"
  ) {
    return undefined;
  }

  const categories = Array.isArray(root.categories)
    ? root.categories
        .map((category) => boundedString(category, 512))
        .filter((category): category is string => category !== undefined)
        .slice(0, 50)
    : [];
  return {
    bio: boundedString(root.bio, 4_096),
    categories,
    city: boundedString(root.city, 256),
    country: boundedString(root.country, 128),
    displayName,
    identityVerified: root.identity_verified,
    isCompany: root.is_company,
    rating,
    soldItemsCount,
    totalRatings,
    username: boundedString(root.username, 256),
  };
}

function normalizeMoney(value: unknown): Money | undefined {
  const root = asObject(value);
  const amount = boundedString(root?.amount, 64);
  const currencyCode = boundedString(root?.currency_code, 8);
  if (!amount || !currencyCode || !/^-?\d+(?:\.\d+)?$/u.test(amount))
    return undefined;
  if (!/^[A-Z]{3}$/u.test(currencyCode)) return undefined;
  return { amount, currency_code: currencyCode };
}

function normalizeLocation(value: unknown): PublicLocation | undefined {
  const root = asObject(value);
  const city = boundedString(root?.city, 250);
  const country = boundedString(root?.country, 250);
  if (!city || !country) return undefined;
  return { city, country, region: boundedString(root?.region, 250) };
}

function normalizeBuyerProtection(value: unknown): BuyerProtection | undefined {
  const root = asObject(value);
  if (!root || typeof root.enabled !== "boolean") return undefined;
  return {
    cap: normalizeMoney(root.cap),
    enabled: root.enabled,
    flatFee: normalizeMoney(root.flat_fee),
    ratePercent: boundedString(root.rate_percent, 32),
  };
}

function cleanListingUrl(
  value: unknown,
  listingId: number,
  origins: Readonly<PublicOrigins>,
): string | undefined {
  const cleaned = cleanListingUrlWithoutKnownId(value, origins);
  if (!cleaned) return undefined;
  const url = new URL(cleaned);
  return url.pathname === `/l/${listingId}` ? cleaned : undefined;
}

function cleanListingUrlWithoutKnownId(
  value: unknown,
  origins: Readonly<PublicOrigins>,
): string | undefined {
  if (typeof value !== "string" || value.length > 2_048) return undefined;
  try {
    const url = new URL(value);
    if (
      url.protocol !== "https:" ||
      url.origin !== origins.listingOrigin ||
      url.username !== "" ||
      url.password !== "" ||
      url.search !== "" ||
      url.hash !== "" ||
      !/^\/l\/[1-9]\d*$/u.test(url.pathname)
    ) {
      return undefined;
    }
    return url.toString();
  } catch {
    return undefined;
  }
}

function cleanImageUrl(
  value: unknown,
  listingId: number,
  origins: Readonly<PublicOrigins>,
): string | undefined {
  if (typeof value !== "string" || value.length > 2_048) return undefined;
  try {
    const url = new URL(value);
    const uuid = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";
    const expected = new RegExp(
      `^/t/listings/${listingId}/thumbs/${uuid}_768x768\\.webp$`,
      "u",
    );
    if (
      url.protocol !== "https:" ||
      url.origin !== origins.imageOrigin ||
      url.username !== "" ||
      url.password !== "" ||
      url.search !== "" ||
      url.hash !== "" ||
      !expected.test(url.pathname)
    ) {
      return undefined;
    }
    return url.toString();
  } catch {
    return undefined;
  }
}

function boundedString(
  value: unknown,
  maximumLength: number,
): string | undefined {
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  if (trimmed.length === 0) return undefined;
  return trimmed.slice(0, maximumLength);
}

function positiveInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0
    ? value
    : undefined;
}

function nonNegativeInteger(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0
    ? value
    : undefined;
}

function finiteNumberInRange(
  value: unknown,
  minimum: number,
  maximum: number,
): number | undefined {
  return typeof value === "number" &&
    Number.isFinite(value) &&
    value >= minimum &&
    value <= maximum
    ? value
    : undefined;
}

function epochMilliseconds(value: unknown): number | undefined {
  return typeof value === "number" && Number.isSafeInteger(value) && value > 0
    ? value
    : undefined;
}

function asObject(value: unknown): JsonObject | undefined {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as JsonObject)
    : undefined;
}

function looksLikeDetail(root: JsonObject): boolean {
  return "id" in root && "seller_id" in root && "starting_price" in root;
}

function firstTextContent(result: CallToolResult): string | undefined {
  for (const block of result.content) {
    if (block.type === "text" && typeof block.text === "string") {
      return block.text.slice(0, 500);
    }
  }
  return undefined;
}
