use crate::cli::SortOrder;
use crate::error::IherbError;
use crate::model::{ProductSummary, SearchResult};
use chromiumoxide::Page;
use scraper::{Html, Selector};

use super::helpers::{
    debug_dump_html, detect_currency_from_html, extract_element_text, parse_price_str,
    parse_review_count,
};

const RESULTS_PER_PAGE: usize = 48;

pub fn build_search_url(
    base_url: &str,
    query: &str,
    sort: SortOrder,
    category: Option<&str>,
    page_num: usize,
) -> String {
    let sort_param = sort.as_url_param();

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
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// Extract search results from a page, trying data attributes first, then __NEXT_DATA__, then DOM text.
pub async fn extract_search(
    page: &Page,
    html: &str,
    query: &str,
    base_url: &str,
    currency: &str,
) -> Result<SearchResult, IherbError> {
    debug_dump_html(html, &format!("search_{}", query.replace(' ', "_")));

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
    let detected_currency = detect_currency_from_html(&doc).unwrap_or_else(|| currency.to_string());

    let mut products = Vec::new();
    let card_sel = Selector::parse("div.product-cell-container").ok();
    let link_sel = Selector::parse("a.absolute-link.product-link, a.product-link").ok();

    if let (Some(card_sel), Some(link_sel)) = (card_sel, link_sel) {
        let cards: Vec<_> = doc.select(&card_sel).collect();
        tracing::debug!("Found {} product-cell-container cards", cards.len());
        products.extend(
            cards
                .iter()
                .filter_map(|card| parse_product_card(card, &link_sel, &detected_currency, base_url)),
        );
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

fn parse_product_card(
    card_el: &scraper::ElementRef,
    link_sel: &Selector,
    currency: &str,
    base_url: &str,
) -> Option<ProductSummary> {
    let link = card_el.select(link_sel).next();
    let link_attrs = link.as_ref().map(|l| l.value());

    let product_id = link_attrs
        .and_then(|a| {
            a.attr("data-product-id")
                .or_else(|| a.attr("data-ga-product-id"))
        })
        .map(|s| s.to_string())
        .or_else(|| {
            link_attrs
                .and_then(|a| a.attr("href"))
                .and_then(extract_id_from_url)
        })?;

    let name = extract_card_attr(card_el, "div.product-title", "content")
        .or_else(|| extract_element_text(card_el, "div.product-title bdi, div.product-title"))
        .or_else(|| {
            link_attrs
                .and_then(|a| a.attr("title"))
                .map(|s| s.to_string())
        })?;

    let brand = link_attrs
        .and_then(|a| a.attr("data-ga-brand-name"))
        .unwrap_or("")
        .to_string();

    let price = extract_card_attr(card_el, "meta[itemprop='price']", "content")
        .and_then(|s| parse_price_str(&s))
        .or_else(|| {
            link_attrs
                .and_then(|a| a.attr("data-ga-discount-price"))
                .and_then(parse_price_str)
        })
        .unwrap_or(0.0);

    let original_price = extract_element_text(card_el, "span.price-olp bdi, span.price-olp")
        .and_then(|s| parse_price_str(&s))
        .filter(|&p| p > price);

    let rating = extract_card_rating(card_el);

    let review_count = extract_element_text(card_el, "a.rating-count span")
        .and_then(|s| parse_review_count(&s));

    let in_stock = extract_card_stock_status(card_el, link_attrs);

    let product_url = link_attrs
        .and_then(|a| a.attr("href"))
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
        currency: currency.to_string(),
        rating,
        review_count,
        product_url,
        product_id,
        in_stock,
    })
}

fn extract_card_rating(card_el: &scraper::ElementRef) -> Option<f64> {
    let sel = Selector::parse("a.stars").ok()?;
    let el = card_el.select(&sel).next()?;
    let title = el.value().attr("title")?;
    title.split('/').next()?.trim().parse::<f64>().ok()
}

fn extract_card_stock_status(
    card_el: &scraper::ElementRef,
    link_attrs: Option<&scraper::node::Element>,
) -> bool {
    Selector::parse("div.product.ga-product, div.product")
        .ok()
        .and_then(|sel| card_el.select(&sel).next())
        .and_then(|el| el.value().attr("data-is-out-of-stock"))
        .map(|s| s.to_lowercase() != "true")
        .or_else(|| {
            link_attrs
                .and_then(|a| a.attr("data-ga-is-out-of-stock"))
                .map(|s| s.to_lowercase() != "true")
        })
        .unwrap_or(true)
}

/// Calculate how many pages needed for the desired limit.
pub fn pages_needed(limit: usize) -> usize {
    limit.div_ceil(RESULTS_PER_PAGE)
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

