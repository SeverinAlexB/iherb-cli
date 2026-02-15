# iherb-cli

A Rust command-line tool for querying product data from [iHerb](https://www.iherb.com). Designed for both AI agents and humans — clean commands, Markdown output, no API key required.

iHerb has no official API. This CLI uses a headless browser to load pages (bypassing Cloudflare), extracts structured data, and presents it in a consistent, parseable format.

## Installation

### Build from source

Requires [Rust](https://www.rust-lang.org/tools/install) (1.70+).

```bash
git clone https://github.com/SeverinAlexB/iherb-cli.git
cd iherb-cli
cargo build --release
```

The binary will be at `target/release/iherb-cli`.

### Browser

iherb-cli needs a Chromium-based browser. It resolves one automatically:

1. User-configured path (`IHERB_BROWSER_PATH` env var or config file)
2. System-installed Chrome/Chromium (auto-detected)
3. Auto-downloads [Chrome for Testing](https://googlechromelabs.github.io/chrome-for-testing/) on first run

## Usage

```
iherb-cli <command> [options] [arguments]
```

### Search for products

```bash
iherb-cli search "vitamin c"
iherb-cli search "omega 3" --limit 20 --sort price-asc
iherb-cli search "protein" --category supplements --sort best-selling
```

**Options:**

| Flag | Description | Default |
|---|---|---|
| `--limit <n>` | Max results to return (paginates automatically) | 20 |
| `--sort <method>` | `relevance`, `price-asc`, `price-desc`, `rating`, `best-selling` | `relevance` |
| `--category <slug>` | Filter by category (e.g., `supplements`, `vitamins`) | — |

**Example output:**

```markdown
## Search results for "vitamin c" (showing 3 of 1,200+)

### 1. California Gold Nutrition, Gold C, Vitamin C, 1,000 mg, 240 Veggie Capsules
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

### Get product details

```bash
iherb-cli product 61864
iherb-cli product https://www.iherb.com/pr/some-product/61864
iherb-cli product 61864 --section ingredients
```

Accepts a numeric product ID or a full iHerb URL.

**Options:**

| Flag | Description |
|---|---|
| `--section <name>` | Show only one section: `overview`, `description`, `ingredients`, `nutrition`, `suggested-use`, `warnings`, `reviews` |

**Example output:**

```markdown
# California Gold Nutrition, Gold C, Vitamin C, 1,000 mg, 240 Veggie Capsules

## Overview
- **Brand:** California Gold Nutrition
- **Price:** $9.60 ~~$12.00~~ (20% off)
- **Rating:** 4.6/5 (12,345 reviews)
- **Availability:** In Stock
- **Product Code:** CGN-01065

## Supplement Facts
| Nutrient | Amount | % Daily Value |
|---|---|---|
| Vitamin C (as L-ascorbic acid) | 1,000 mg | 1,111% |

- **Serving Size:** 1 Veggie Capsule
- **Servings Per Container:** 240

## Suggested Use
Take 1 capsule daily with or without food.
```

### Global flags

| Flag | Description | Default |
|---|---|---|
| `--country <code>` | Country code for localized pricing (e.g., `us`, `ch`, `de`) | `us` |
| `--currency <code>` | Currency code (e.g., `USD`, `CHF`, `EUR`) | `USD` |
| `--no-cache` | Bypass local cache and fetch fresh data | — |
| `--delay <ms>` | Delay between requests in milliseconds | `2000` |
| `--debug` | Run browser in headed (visible) mode | — |

```bash
# Swiss storefront with CHF pricing
iherb-cli search "magnesium" --country ch --currency CHF

# Fast mode (shorter delay between requests)
iherb-cli search "zinc" --delay 500

# Debug with visible browser
iherb-cli product 61864 --debug
```

## Configuration

Settings are resolved in order of priority:

1. CLI flags (highest)
2. Environment variables (`IHERB_BROWSER_PATH`, `IHERB_COUNTRY`, `IHERB_CURRENCY`)
3. Config file
4. Defaults

### Config file

Location: `~/.config/iherb-cli/config.toml`

```toml
[defaults]
country = "ch"
currency = "CHF"
```

## Caching

Scraped data is cached locally to `~/.cache/iherb-cli/` to reduce redundant requests.

All cached data expires after **30 days**.

Every result includes a `Data from:` timestamp so you know how fresh the data is. Use `--no-cache` to bypass the cache and fetch fresh data.

## How it works

iHerb uses Cloudflare anti-bot protection, so simple HTTP requests are blocked. iherb-cli uses a headless Chromium browser (via the Chrome DevTools Protocol) to load pages like a real user.

**Data extraction** uses multiple strategies with automatic fallback:

1. **JSON-LD** structured data embedded in the page
2. **JavaScript globals** (`window.PRODUCT_DETAILS`, etc.)
3. **`__NEXT_DATA__`** — iHerb's Next.js server-side data
4. **DOM scraping** — CSS selector-based extraction as a last resort

This layered approach keeps the tool working even when iHerb changes their page structure.

## Claude Code skill

This repo includes a [Claude Code skill](https://code.claude.com/docs/en/skills) that teaches AI agents how to use `iherb-cli` for supplement research. With the skill installed, Claude can autonomously search for products, compare ingredients, and make recommendations.

### Install the skill

```bash
/install-plugin iherb-agent@SeverinAlexB/iherb-cli
```

### What the agent can do

Once installed, Claude can handle requests like:

- *"What's the best-rated vitamin D3 on iHerb?"*
- *"Compare the top 3 magnesium glycinate supplements by price and dosage"*
- *"What are the ingredients in iHerb product 61864?"*
- *"Find me a budget omega-3 supplement with good reviews"*

The skill guides Claude through multi-step workflows — searching, fetching details, comparing nutrition facts, and recommending products with reasoning.

### Requirements

The `iherb-cli` binary must be available on `PATH`. Build it first:

```bash
cargo build --release
export PATH="$PATH:$(pwd)/target/release"
```

## Architecture

```
src/
├── main.rs              # Entry point, command orchestration
├── cli.rs               # Argument parsing (clap)
├── model.rs             # Data structures (ProductSummary, ProductDetail)
├── error.rs             # Error types
├── config.rs            # Configuration loading
├── cache.rs             # File-based JSON caching
├── output.rs            # Markdown formatting
├── browser/
│   ├── session.rs       # Browser lifecycle & stealth mode
│   ├── resolve.rs       # Chrome binary detection
│   └── download.rs      # Chrome for Testing auto-download
└── scraper/
    ├── navigation.rs    # Page loading, Cloudflare handling, retries
    ├── helpers.rs       # Price parsing, text extraction utilities
    ├── search.rs        # Search result extraction
    ├── product.rs       # Product detail extraction
    └── extract.rs       # JSON-LD / __NEXT_DATA__ / JS global extraction
```

## Tech stack

| Component | Library |
|---|---|
| CLI framework | [clap](https://docs.rs/clap) (derive API) |
| Browser automation | [chromiumoxide](https://docs.rs/chromiumoxide) (CDP) |
| Async runtime | [tokio](https://docs.rs/tokio) |
| HTML parsing | [scraper](https://docs.rs/scraper) |
| JSON | [serde](https://docs.rs/serde) + [serde_json](https://docs.rs/serde_json) |
| Config | [toml](https://docs.rs/toml) + [dirs](https://docs.rs/dirs) |
| Error handling | [thiserror](https://docs.rs/thiserror) + [anyhow](https://docs.rs/anyhow) |

## Supported countries

45+ country codes including: `us`, `ca`, `au`, `nz`, `gb`, `de`, `fr`, `ch`, `at`, `it`, `es`, `nl`, `be`, `se`, `no`, `dk`, `fi`, `jp`, `kr`, `cn`, `tw`, `hk`, `sg`, `my`, `th`, `in`, `ae`, `sa`, `il`, `br`, `mx`, `cl`, `co`, `ar`, and more.

## License

MIT
