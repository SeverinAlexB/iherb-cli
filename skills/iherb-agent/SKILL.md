---
name: iherb-agent
description: Query supplement and health product data from iHerb using the iherb-cli command-line tool. Use when the user asks about supplements, vitamins, health products, or anything related to iHerb — including searching for products, comparing options, checking ingredients or nutrition facts, finding prices, reading review summaries, or getting purchase recommendations. Triggers include questions like "What's the best vitamin C?", "Find me a magnesium supplement", "What are the ingredients in this product?", "Compare omega-3 options on iHerb", or "How much does ashwagandha cost?".
---

# iherb-agent

Use the `iherb-cli` binary to query iHerb product data. It uses a headless browser (first run may take a moment to download Chrome). Results are cached for 30 days. Every result includes a `Data from:` timestamp — use `--no-cache` if the data is stale.

## Commands

### Search

```bash
iherb-cli search "<query>" [--limit <n>] [--sort <method>] [--category <slug>]
```

- `--limit`: max results (default 20)
- `--sort`: `relevance` (default), `price-asc`, `price-desc`, `rating`, `best-selling`
- `--category`: filter by category slug (e.g., `supplements`, `vitamins`, `protein`)

Output: Markdown list with name, brand, price, rating, review count, product ID, URL.

### Product details

```bash
iherb-cli product <id-or-url> [--section <name>]
```

Accepts a numeric product ID (e.g., `61864`) or full URL.

`--section` options: `overview`, `description`, `ingredients`, `nutrition`, `suggested-use`, `warnings`, `reviews`

Output: Full Markdown with overview, supplement facts table, ingredients, suggested use, warnings, review distribution.

### Global flags

- `--country <code>`: localized storefront (e.g., `ch`, `de`, `jp`). Default: `us`
- `--currency <code>`: currency (e.g., `CHF`, `EUR`). Default: `USD`
- `--no-cache`: bypass cache
- `--debug`: show browser window

## Workflows

### Find the best product for a need

1. Search with `--sort best-selling` or `--sort rating` to find top options
2. Get details on 2-3 top candidates: `iherb-cli product <id>`
3. Compare ingredients, dosage, price-per-serving, and ratings
4. Recommend with reasoning

```bash
iherb-cli search "vitamin d3" --limit 20 --sort best-selling
iherb-cli product 53330
iherb-cli product 18222
```

### Compare products

1. Get details for each product
2. Extract and compare: active ingredients, dosage per serving, servings per container, price per serving, rating, form (capsule/tablet/liquid)

```bash
iherb-cli product 53330 --section nutrition
iherb-cli product 18222 --section nutrition
```

Calculate price-per-serving: price / servings_per_container.

### Check specific product info

Use `--section` to fetch only what's needed:

```bash
iherb-cli product 61864 --section ingredients   # what's in it
iherb-cli product 61864 --section nutrition      # supplement facts table
iherb-cli product 61864 --section reviews        # rating breakdown
```

### Find budget options

```bash
iherb-cli search "magnesium glycinate" --sort price-asc --limit 20
```

Then verify quality by checking ingredients and ratings on the cheapest options.

### Localized pricing

```bash
iherb-cli search "omega 3" --country ch --currency CHF
iherb-cli product 61864 --country de --currency EUR
```

## Tips

- Search queries work best with specific supplement names (e.g., "magnesium glycinate" not just "magnesium")
- Always check the supplement facts table when comparing — brand marketing can be misleading
- Review count matters as much as rating — 4.5 stars with 10,000 reviews is more reliable than 5.0 with 12
- When recommending, mention form (capsule, tablet, powder, liquid), dosage, servings per container, and price per serving
- First run downloads a browser binary — warn the user it may take a moment
