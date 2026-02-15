# iherb-cli — Requirements Specification

## 1. Project Overview

**iherb-cli** is a Rust-based command-line tool that provides an easy, effective interface for AI agents (and humans) to query product data from [iHerb](https://www.iherb.com), a supplement and health products web shop.

The CLI is inspired by the GitHub CLI (`gh`) — clean, human-readable commands with structured output that agents can parse reliably.

### Why this exists

There is no official iHerb API. This CLI acts as a structured interface on top of iHerb's website, extracting product data via a headless browser and presenting it in a consistent, agent-friendly format.

---

## 2. CLI Interface

**Binary name:** `iherb-cli`

**Command style:** Subcommand-based (like `gh`).

```
iherb-cli <command> [options] [arguments]
```

### Global flags

| Flag | Description |
|---|---|
| `--country <code>` | Country code for localized pricing/availability (e.g., `us`, `ch`, `de`) |
| `--currency <code>` | Currency code (e.g., `USD`, `CHF`, `EUR`) |
| `--no-cache` | Bypass the local cache and fetch fresh data |
| `--delay <ms>` | Delay between requests in milliseconds (default: 2000) |
| `--debug` | Run browser in headed mode for troubleshooting |
| `--help` | Show help |
| `--version` | Show version |

---

## 3. Commands

### 3.1 `search` — Search for products

```
iherb-cli search <query> [opti
ons]
```

**Arguments:**
- `<query>` — Search term (e.g., `"vitamin c"`, `"omega 3"`, `"ashwagandha"`)

**Options:**
| Flag | Description |
|---|---|
| `--limit <n>` | Max number of results to return (default: 10). Automatically paginates behind the scenes if needed. |
| `--sort <method>` | Sort order: `relevance` (default), `price-asc`, `price-desc`, `rating`, `best-selling` |
| `--category <slug>` | Filter by category (e.g., `supplements`, `vitamins`, `protein`) |

**Output:** A Markdown-formatted list of matching products with summary info:
- Product name
- Brand
- Price (current + original if discounted)
- Rating (stars + review count)
- Product URL
- Product ID (for use with the `product` command)

**Example output:**
```markdown
## Search results for "vitamin c" (showing 3 of 1,200+)

### 1. California Gold Nutrition, Gold C, USP Grade Vitamin C, 1,000 mg, 240 Veggie Capsules
- **Brand:** California Gold Nutrition
- **Price:** $9.60 ~~$12.00~~
- **Rating:** 4.6/5 (12,345 reviews)
- **ID:** 61864
- **URL:** https://www.iherb.com/pr/california-gold-nutrition-gold-c-1000-mg-240-veggie-capsules/61864

---

### 2. Now Foods, C-1000, 250 Tablets
- **Brand:** Now Foods
- **Price:** $11.85
- **Rating:** 4.7/5 (8,901 reviews)
- **ID:** 479
- **URL:** https://www.iherb.com/pr/now-foods-c-1000-250-tablets/479
```

### 3.2 `product` — Get detailed product info

```
iherb-cli product <id-or-url> [options]
```

**Arguments:**
- `<id-or-url>` — Either a numeric product ID (e.g., `61864`) or a full iHerb product URL

**Options:**
| Flag | Description |
|---|---|
| `--section <name>` | Only show a specific section: `overview`, `ingredients`, `nutrition`, `reviews` |

**Output:** Full Markdown-formatted product detail including:

- **Overview:** Name, brand, price, rating, availability, product code, description
- **Ingredients:** Full ingredient list
- **Supplement Facts / Nutrition:** Serving size, servings per container, full nutritional breakdown table
- **Suggested Use:** Dosage instructions
- **Warnings:** Any warnings or cautions
- **Reviews summary:** Average rating, total count, rating distribution

**Example output:**
```markdown
# California Gold Nutrition, Gold C, USP Grade Vitamin C, 1,000 mg, 240 Veggie Capsules

## Overview
- **Brand:** California Gold Nutrition
- **Price:** $9.60 ~~$12.00~~ (20% off)
- **Rating:** 4.6/5 (12,345 reviews)
- **Availability:** In Stock
- **Product Code:** CGN-01065
- **Shipping Weight:** 0.55 lb

## Description
Gold C Vitamin C is USP grade L-ascorbic acid — a high-quality form of vitamin C.
Supports immune health and provides antioxidant protection.

## Supplement Facts
| Nutrient | Amount | % Daily Value |
|---|---|---|
| Vitamin C (as L-ascorbic acid) | 1,000 mg | 1,111% |

- **Serving Size:** 1 Veggie Capsule
- **Servings Per Container:** 240

## Other Ingredients
Modified cellulose (vegetarian capsule), rice flour, magnesium stearate, silica.

## Suggested Use
Take 1 capsule daily with or without food.

## Warnings
Store in a cool, dry place. Keep out of reach of children.

## Reviews
- **Average:** 4.6/5
- **Total:** 12,345 reviews
- 5 stars: 72%
- 4 stars: 18%
- 3 stars: 6%
- 2 stars: 2%
- 1 star: 2%
```

---

## 4. Data Fields

The following data fields should be extracted from iHerb product pages:

### Product summary (used in search results)
- Product name/title
- Brand
- Current price
- Original/list price (if discounted)
- Currency
- Rating (average stars)
- Review count
- Product URL
- Product ID
- Availability (in stock / out of stock)

### Product detail (used in `product` command)
All of the above, plus:
- Description text
- Product code / SKU
- UPC code
- Ingredient list
- Supplement facts / nutrition table
- Suggested use
- Warnings / cautions
- Shipping weight
- Category breadcrumb
- Review distribution (star breakdown)

---

## 5. Output Format

All output is **Markdown-formatted** by default. This is both human-readable in a terminal and easy for AI agents to parse.

### Design principles for output:
- Consistent heading structure (H1 for product name, H2 for sections)
- Tables for structured data (supplement facts, comparisons)
- Bold labels for key-value pairs
- Strikethrough for original prices when discounted
- Horizontal rules to separate search results
- No unnecessary decoration or color codes

---

## 6. Scraping Architecture

### 6.1 Headless browser

iHerb uses Cloudflare anti-bot protection, so simple HTTP requests are blocked. The CLI uses a **headless browser** (Chromium via CDP — Chrome DevTools Protocol) to load pages.

**Browser resolution strategy (in order):**
1. Check for a user-configured browser path (env var `IHERB_BROWSER_PATH` or config file)
2. Auto-detect an installed Chrome/Chromium on the system
3. If none found, auto-download a Chromium binary to a local data directory on first run

### 6.2 Data extraction strategy

Preferred extraction methods (in order of preference):
1. **`__NEXT_DATA__`** — iHerb runs on Next.js, so product pages likely embed full data as serialized JSON in a `<script id="__NEXT_DATA__">` tag. This is the most reliable extraction method.
2. **Internal XHR/API responses** — Intercept network requests the frontend makes to internal APIs and capture the JSON responses.
3. **DOM scraping** — As a fallback, extract data from HTML elements using CSS selectors.

### 6.3 Rate limiting & request behavior
- **Default delay of 2 seconds** between requests — safe for Cloudflare, no tuning needed by agents
- Configurable via `--delay <ms>` flag for power users (e.g., `--delay 500` for faster, `--delay 5000` for safer)
- Also configurable in config file: `delay_ms = 2000`
- Set realistic browser headers and viewport
- Run the browser in headless mode by default
- Support `--debug` flag to run the browser in headed mode for troubleshooting

---

## 7. Caching

The CLI caches scraped data locally to reduce redundant requests and speed up repeated queries.

**Cache location:** Platform-appropriate data directory (e.g., `~/.cache/iherb-cli/` on Linux/macOS)

**Cache behavior:**
- **Product data** is cached by product ID with a configurable TTL (default: 24 hours)
- **Search results** are cached by query + options hash with a shorter TTL (default: 1 hour)
- `--no-cache` flag bypasses the cache for any command
- Cache is stored as local files (JSON or SQLite)

---

## 8. Localization

iHerb operates localized storefronts via subdomains (e.g., `ch.iherb.com`, `de.iherb.com`).

**Configuration priority:**
1. Command-line flags (`--country`, `--currency`)
2. Config file (`~/.config/iherb-cli/config.toml`)
3. Sensible defaults (`us` / `USD`)

**Config file example:**
```toml
[defaults]
country = "ch"
currency = "CHF"
```

---

## 9. Technical Stack

| Component | Choice |
|---|---|
| Language | Rust |
| CLI framework | `clap` (derive API) |
| Headless browser | `chromiumoxide` (async CDP-based, with `chromiumoxide_stealth` for anti-bot) |
| Async runtime | `tokio` |
| HTTP client | `reqwest` (for non-browser requests, e.g., sitemap fetching) |
| HTML parsing | `scraper` (CSS selectors) |
| JSON handling | `serde` + `serde_json` |
| Config | `toml` + `dirs` (for platform-appropriate paths) |
| Cache storage | File-based JSON or `rusqlite` (SQLite) |

---

## 10. Non-Functional Requirements

### Performance
- Search results should return within 10 seconds (headless browser overhead)
- Cached responses should return instantly (< 100ms)
- The CLI binary should be a single self-contained executable (minus the browser dependency)

### Reliability
- Graceful error messages when iHerb is unreachable or blocks the request
- Retry logic for transient failures (configurable, default: 2 retries)
- Clear messaging when the browser cannot be found or downloaded

### Usability
- Helpful `--help` text for every command and flag
- Sensible defaults so the simplest command works: `iherb-cli search "vitamin c"`
- Error messages should suggest fixes (e.g., "Browser not found. Install Chrome or set IHERB_BROWSER_PATH")

### Maintainability
- Scraping logic should be isolated from CLI/output logic (so selectors can be updated independently when iHerb changes their HTML)
- Clear module separation: `cli`, `scraper`, `cache`, `output`, `config`

---

## 11. Out of Scope (for v1)

- User authentication / account features
- Placing orders or adding to cart
- Review text scraping (only summary/distribution)
- Price history tracking
- Comparison command
- Proxy support
- Notifications / price alerts

---

## 12. Resolved Decisions

1. **Browser auto-download:** Uses **Chrome for Testing** — Google's official automation-focused Chromium build. Versioned downloads available via their JSON API at `googlechromelabs.github.io/chrome-for-testing/`. This is the same distribution Playwright and Puppeteer use.
