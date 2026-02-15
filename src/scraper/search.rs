use crate::error::IherbError;
use crate::model::{ProductSummary, SearchResult};
use chromiumoxide::Page;
use scraper::{Html, Selector};

const RESULTS_PER_PAGE: usize = 48;

pub fn build_search_url(
    base_url: &str,
    query: &str,
    sort: &str,
    category: Option<&str>,
    page_num: usize,
) -> String {
    let sort_param = match sort {
        "price-asc" => "&sr=4",
        "price-desc" => "&sr=3",
        "rating" => "&sr=1",
        "best-selling" => "&sr=2",
        _ => "", // relevance is default
    };

    let category_param = match category {
        Some(cat) => format!("&cids={}", cat),
        None => String::new(),
    };

    let page_param = if page_num > 1 {
        format!("&p={}", page_num)
    } else {
        String::new()
    };

    format!(
        "{}/search?kw={}{}{}{}",
        base_url,
        urlencoded(query),
        sort_param,
        category_param,
        page_param
    )
}

fn urlencoded(s: &str) -> String {
    s.replace(' ', "+")
        .replace('&', "%26")
        .replace('=', "%3D")
        .replace('#', "%23")
}

/// Extract search results from a page, trying data attributes first, then __NEXT_DATA__, then DOM text.
pub async fn extract_search(
    page: &Page,
    html: &str,
    query: &str,
    base_url: &str,
    currency: &str,
) -> Result<SearchResult, IherbError> {
    // Dump HTML for debugging
    if tracing::enabled!(tracing::Level::DEBUG) {
        let dump_path = format!("/tmp/iherb_search_{}.html", query.replace(' ', "_"));
        let _ = std::fs::write(&dump_path, html);
        tracing::debug!("Dumped search HTML to {}", dump_path);
    }

    // Try __NEXT_DATA__ first (may exist on some page versions)
    if let Ok(Some(next_data)) = super::extract::extract_next_data(page).await {
        tracing::debug!("Attempting __NEXT_DATA__ extraction for search");
        if let Some(result) = parse_search_from_next_data(&next_data, query, base_url) {
            tracing::info!("Successfully extracted search results from __NEXT_DATA__");
            return Ok(result);
        }
        tracing::warn!("__NEXT_DATA__ search extraction failed, falling back to DOM");
    }

    tracing::info!("Extracting search results from DOM");
    parse_search_from_html(html, query, base_url, currency)
}

/// Parse search results from __NEXT_DATA__ JSON.
pub fn parse_search_from_next_data(
    data: &serde_json::Value,
    query: &str,
    base_url: &str,
) -> Option<SearchResult> {
    let props = data.get("props")?.get("pageProps")?;

    let products_arr = props
        .get("products")
        .or_else(|| props.get("searchResults"))
        .or_else(|| props.get("items"))
        .and_then(|v| v.as_array())?;

    let total = props
        .get("totalResults")
        .or_else(|| props.get("totalCount"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let products: Vec<ProductSummary> = products_arr
        .iter()
        .filter_map(|item| parse_product_summary_json(item, base_url))
        .collect();

    if products.is_empty() {
        return None;
    }

    Some(SearchResult {
        query: query.to_string(),
        total_results: total,
        products,
    })
}

fn parse_product_summary_json(item: &serde_json::Value, base_url: &str) -> Option<ProductSummary> {
    let name = item
        .get("title")
        .or_else(|| item.get("name"))
        .and_then(|v| v.as_str())?
        .to_string();

    let brand = item
        .get("brandName")
        .or_else(|| {
            item.get("brand")
                .and_then(|b| b.get("name"))
                .or_else(|| item.get("brand"))
        })
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let product_id = item
        .get("id")
        .or_else(|| item.get("productId"))
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_u64().map(|n| n.to_string()))
        })?;

    let price = item
        .get("price")
        .or_else(|| item.get("discountPrice"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let original_price = item
        .get("listPrice")
        .or_else(|| item.get("retailPrice"))
        .and_then(|v| v.as_f64())
        .filter(|&p| p > price);

    let currency = item
        .get("currency")
        .and_then(|v| v.as_str())
        .unwrap_or("USD")
        .to_string();

    let rating = item.get("rating").and_then(|v| v.as_f64());
    let review_count = item
        .get("reviewCount")
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let in_stock = item
        .get("inStock")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let product_url = item
        .get("url")
        .or_else(|| item.get("productUrl"))
        .and_then(|v| v.as_str())
        .map(|u| {
            if u.starts_with("http") {
                u.to_string()
            } else {
                format!("{}{}", base_url, u)
            }
        })
        .unwrap_or_else(|| format!("{}/pr/p/{}", base_url, product_id));

    Some(ProductSummary {
        name,
        brand,
        price,
        original_price,
        currency,
        rating,
        review_count,
        product_url,
        product_id,
        in_stock,
    })
}

/// Parse search results from HTML using data attributes and CSS selectors.
pub fn parse_search_from_html(
    html: &str,
    query: &str,
    base_url: &str,
    currency: &str,
) -> Result<SearchResult, IherbError> {
    let doc = Html::parse_document(html);

    let total_results = extract_total_results(&doc);

    // Detect actual currency from the page, falling back to config currency
    let detected_currency = detect_currency_from_html(&doc).unwrap_or_else(|| currency.to_string());

    let mut products = Vec::new();

    // Primary strategy: Find product-cell-container cards and extract data from
    // the <a class="absolute-link product-link"> inside each card, which carries
    // rich data-ga-* attributes (brand, price, product ID, etc.)
    let card_sel = Selector::parse("div.product-cell-container").ok();
    let link_sel = Selector::parse("a.absolute-link.product-link, a.product-link").ok();

    if let (Some(card_sel), Some(link_sel)) = (card_sel, link_sel) {
        let cards: Vec<_> = doc.select(&card_sel).collect();
        tracing::debug!("Found {} product-cell-container cards", cards.len());

        for card_el in &cards {
            // Find the product link with data attributes
            let link = card_el.select(&link_sel).next();
            let link_attrs = link.as_ref().map(|l| l.value());

            // Product ID from data attributes or URL
            let product_id = link_attrs
                .and_then(|a| {
                    a.attr("data-product-id")
                        .or_else(|| a.attr("data-ga-product-id"))
                })
                .map(|s| s.to_string())
                .or_else(|| {
                    link_attrs
                        .and_then(|a| a.attr("href"))
                        .and_then(|url| extract_id_from_url(url))
                });

            // Title from .product-title content attr or bdi text
            let name = extract_card_attr(card_el, "div.product-title", "content")
                .or_else(|| {
                    extract_card_text_inner(card_el, "div.product-title bdi, div.product-title")
                })
                .or_else(|| {
                    link_attrs
                        .and_then(|a| a.attr("title"))
                        .map(|s| s.to_string())
                });

            // Brand from data-ga-brand-name on the link
            let brand = link_attrs
                .and_then(|a| a.attr("data-ga-brand-name"))
                .map(|s| s.to_string());

            // Price from meta[itemprop="price"] or data-ga-discount-price
            let price = extract_card_attr(card_el, "meta[itemprop='price']", "content")
                .and_then(|s| parse_price(&s))
                .or_else(|| {
                    link_attrs
                        .and_then(|a| a.attr("data-ga-discount-price"))
                        .and_then(|s| parse_price(s))
                })
                .unwrap_or(0.0);

            // Original price from .price-olp
            let original_price =
                extract_card_text_inner(card_el, "span.price-olp bdi, span.price-olp")
                    .and_then(|s| parse_price(&s))
                    .filter(|&p| p > price);

            // Rating from a.stars title attribute (format: "4.8/5 - 373,798 Reviews")
            let rating = card_el
                .select(
                    &Selector::parse("a.stars").unwrap_or_else(|_| Selector::parse("a").unwrap()),
                )
                .next()
                .and_then(|el| el.value().attr("title"))
                .and_then(|title| title.split('/').next()?.trim().parse::<f64>().ok());

            // Review count from a.rating-count span
            let review_count =
                extract_card_text_inner(card_el, "a.rating-count span").and_then(|s| {
                    s.replace(',', "")
                        .chars()
                        .filter(|c| c.is_ascii_digit())
                        .collect::<String>()
                        .parse::<u32>()
                        .ok()
                });

            // Stock status
            let in_stock = card_el
                .select(
                    &Selector::parse("div.product.ga-product, div.product")
                        .unwrap_or_else(|_| Selector::parse("div").unwrap()),
                )
                .next()
                .and_then(|el| el.value().attr("data-is-out-of-stock"))
                .map(|s| s.to_lowercase() != "true")
                .or_else(|| {
                    link_attrs
                        .and_then(|a| a.attr("data-ga-is-out-of-stock"))
                        .map(|s| s.to_lowercase() != "true")
                })
                .unwrap_or(true);

            // Product URL
            let product_url = link_attrs
                .and_then(|a| a.attr("href"))
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    product_id
                        .as_ref()
                        .map(|id| format!("{}/pr/p/{}", base_url, id))
                        .unwrap_or_default()
                });

            if let (Some(name), Some(id)) = (name, product_id) {
                products.push(ProductSummary {
                    name,
                    brand: brand.unwrap_or_default(),
                    price,
                    original_price,
                    currency: detected_currency.clone(),
                    rating,
                    review_count,
                    product_url,
                    product_id: id,
                    in_stock,
                });
            }
        }
    }

    if !products.is_empty() {
        tracing::info!("Extracted {} products from search DOM", products.len());
    } else {
        tracing::warn!("No products extracted from search DOM");
    }

    Ok(SearchResult {
        query: query.to_string(),
        total_results,
        products,
    })
}

/// Calculate how many pages needed for the desired limit.
pub fn pages_needed(limit: usize) -> usize {
    (limit + RESULTS_PER_PAGE - 1) / RESULTS_PER_PAGE
}

fn extract_card_attr(el: &scraper::ElementRef, selector: &str, attr: &str) -> Option<String> {
    let sel = Selector::parse(selector).ok()?;
    let child = el.select(&sel).next()?;
    child
        .value()
        .attr(attr)
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn extract_card_text_inner(el: &scraper::ElementRef, selectors: &str) -> Option<String> {
    for sel_str in selectors.split(',') {
        if let Ok(sel) = Selector::parse(sel_str.trim()) {
            if let Some(child) = el.select(&sel).next() {
                let text: String = child
                    .text()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn extract_id_from_url(url: &str) -> Option<String> {
    url.split('/')
        .rev()
        .find(|segment| segment.chars().all(|c| c.is_ascii_digit()) && !segment.is_empty())
        .map(|s| s.to_string())
}

fn extract_total_results(doc: &Html) -> Option<u32> {
    // Best source: hidden span#product-count with data-count attribute
    if let Ok(sel) = Selector::parse("span#product-count") {
        if let Some(el) = doc.select(&sel).next() {
            if let Some(count) = el.value().attr("data-count") {
                if let Ok(n) = count.replace(',', "").parse::<u32>() {
                    if n > 0 {
                        return Some(n);
                    }
                }
            }
        }
    }

    // Fallback: parse "1 - 48 of 12,008 results for" text
    let sel_strs = ["div.sub-sort-title.display-items", ".display-items"];

    for sel_str in &sel_strs {
        if let Ok(sel) = Selector::parse(sel_str) {
            if let Some(el) = doc.select(&sel).next() {
                let text: String = el.text().collect();
                if let Some(idx) = text.find("of ") {
                    let after = &text[idx + 3..];
                    let num: String = after
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == ',')
                        .collect::<String>()
                        .replace(',', "");
                    if let Ok(n) = num.parse::<u32>() {
                        if n > 0 {
                            return Some(n);
                        }
                    }
                }
            }
        }
    }

    None
}

/// Detect the actual currency from the page via meta tags or price text.
pub fn detect_currency_from_html(doc: &Html) -> Option<String> {
    // Try meta[itemprop="priceCurrency"]
    if let Ok(sel) = Selector::parse("meta[itemprop='priceCurrency']") {
        if let Some(el) = doc.select(&sel).next() {
            if let Some(code) = el.value().attr("content") {
                let code = code.trim().to_uppercase();
                if !code.is_empty() {
                    tracing::debug!("Detected currency from meta tag: {}", code);
                    return Some(code);
                }
            }
        }
    }

    // Try to detect from price text (e.g., "CHF 4.46", "$4.46", "€4.46")
    if let Ok(sel) = Selector::parse("span.price bdi, div.price bdi, .product-price bdi") {
        if let Some(el) = doc.select(&sel).next() {
            let text: String = el.text().collect::<Vec<_>>().join("").trim().to_string();
            if let Some(currency) = detect_currency_from_text(&text) {
                tracing::debug!("Detected currency from price text: {}", currency);
                return Some(currency);
            }
        }
    }

    None
}

fn detect_currency_from_text(text: &str) -> Option<String> {
    let text = text.trim();
    if text.starts_with('$') {
        Some("USD".to_string())
    } else if text.starts_with('€') {
        Some("EUR".to_string())
    } else if text.starts_with('£') {
        Some("GBP".to_string())
    } else if text.starts_with("CHF") {
        Some("CHF".to_string())
    } else if text.starts_with("CA$") || text.starts_with("C$") {
        Some("CAD".to_string())
    } else if text.starts_with("A$") || text.starts_with("AU$") {
        Some("AUD".to_string())
    } else if text.starts_with("¥") {
        Some("JPY".to_string())
    } else if text.starts_with("₩") {
        Some("KRW".to_string())
    } else {
        None
    }
}

fn parse_price(s: &str) -> Option<f64> {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    cleaned.parse().ok()
}
