use crate::error::IherbError;
use crate::model::{Nutrient, ProductDetail, ReviewDistribution, SupplementFacts};
use chromiumoxide::Page;
use scraper::{Html, Selector};

/// Extract product detail from a page, trying JSON-LD first, then JS globals, then DOM.
pub async fn extract_product(
    page: &Page,
    html: &str,
    product_id: &str,
    base_url: &str,
    currency: &str,
) -> Result<ProductDetail, IherbError> {
    // Dump HTML for debugging
    if tracing::enabled!(tracing::Level::DEBUG) {
        let dump_path = format!("/tmp/iherb_product_{}.html", product_id);
        let _ = std::fs::write(&dump_path, html);
        tracing::debug!("Dumped HTML to {}", dump_path);
    }

    // Try JSON-LD first (most reliable structured data)
    if let Some(json_ld) = super::extract::extract_json_ld(html) {
        tracing::debug!("Attempting JSON-LD extraction for product {}", product_id);
        if let Some(mut product) = parse_from_json_ld(&json_ld, product_id, base_url) {
            // JSON-LD has core fields; enrich with DOM-only fields
            enrich_from_html(html, &mut product);
            tracing::info!("Successfully extracted product from JSON-LD + DOM enrichment");
            return Ok(product);
        }
        tracing::warn!("JSON-LD extraction failed, trying JS globals");
    }

    // Try JS globals
    if let Ok(Some(globals)) = super::extract::extract_js_globals(page).await {
        tracing::debug!(
            "Attempting JS globals extraction for product {}",
            product_id
        );
        if let Some(mut product) = parse_from_js_globals(&globals, product_id, base_url, currency) {
            enrich_from_html(html, &mut product);
            tracing::info!("Successfully extracted product from JS globals + DOM enrichment");
            return Ok(product);
        }
        tracing::warn!("JS globals extraction failed, trying __NEXT_DATA__");
    }

    // Try __NEXT_DATA__
    if let Ok(Some(next_data)) = super::extract::extract_next_data(page).await {
        tracing::debug!(
            "Attempting __NEXT_DATA__ extraction for product {}",
            product_id
        );
        if let Some(product) = parse_from_next_data(&next_data, product_id, base_url) {
            tracing::info!("Successfully extracted product from __NEXT_DATA__");
            return Ok(product);
        }
        tracing::warn!("__NEXT_DATA__ extraction failed, falling back to DOM");
    }

    // Fallback to DOM scraping
    tracing::info!("Extracting product from DOM for {}", product_id);
    parse_from_html(html, product_id, base_url, currency)
}

/// Parse product from JSON-LD structured data.
fn parse_from_json_ld(
    data: &serde_json::Value,
    product_id: &str,
    base_url: &str,
) -> Option<ProductDetail> {
    let name = data.get("name").and_then(|v| v.as_str())?.to_string();

    if name.is_empty() {
        return None;
    }

    let brand = data
        .get("brand")
        .and_then(|b| {
            b.get("name")
                .and_then(|v| v.as_str())
                .or_else(|| b.as_str())
        })
        .unwrap_or("")
        .to_string();

    let offers = data.get("offers");
    let price = offers
        .and_then(|o| o.get("price"))
        .and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| v.as_f64())
        })
        .unwrap_or(0.0);

    let currency = offers
        .and_then(|o| o.get("priceCurrency"))
        .and_then(|v| v.as_str())
        .unwrap_or("USD")
        .to_string();

    let in_stock = offers
        .and_then(|o| o.get("availability"))
        .and_then(|v| v.as_str())
        .map(|s| s.contains("InStock"))
        .unwrap_or(true);

    let agg = data.get("aggregateRating");
    let rating = agg.and_then(|a| {
        a.get("ratingValue").and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| v.as_f64())
        })
    });
    let review_count = agg.and_then(|a| {
        a.get("reviewCount").and_then(|v| {
            v.as_str()
                .and_then(|s| s.parse::<u32>().ok())
                .or_else(|| v.as_u64().map(|n| n as u32))
        })
    });

    let description = data
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let product_code = data
        .get("sku")
        .or_else(|| data.get("mpn"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let upc = data
        .get("gtin12")
        .or_else(|| data.get("gtin13"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let product_url = data
        .get("url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{}/pr/p/{}", base_url, product_id));

    Some(ProductDetail {
        name,
        brand,
        price,
        original_price: None, // JSON-LD typically only has current price
        currency,
        rating,
        review_count,
        product_url,
        product_id: product_id.to_string(),
        in_stock,
        description,
        product_code,
        upc,
        ingredients: None,      // enriched from DOM
        supplement_facts: None, // enriched from DOM
        suggested_use: None,    // enriched from DOM
        warnings: None,         // enriched from DOM
        shipping_weight: None,  // enriched from DOM
        category_breadcrumb: None,
        review_distribution: None, // enriched from DOM
    })
}

/// Parse product from JS globals (window.PRODUCT_DETAILS, window.IHR_DL).
fn parse_from_js_globals(
    globals: &serde_json::Value,
    product_id: &str,
    base_url: &str,
    currency: &str,
) -> Option<ProductDetail> {
    let pd = globals.get("productDetails");
    let ihr = globals.get("ihrProduct");

    let name = ihr
        .and_then(|p| p.get("prdNm"))
        .or_else(|| pd.and_then(|p| p.get("name")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if name.is_empty() {
        return None;
    }

    let brand = ihr
        .and_then(|p| p.get("brndNm"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let price_str = ihr
        .and_then(|p| p.get("prc"))
        .and_then(|v| v.as_str())
        .unwrap_or("0");
    let price = parse_price_str(price_str).unwrap_or(0.0);

    let product_code = pd
        .and_then(|p| p.get("code"))
        .or_else(|| ihr.and_then(|p| p.get("prtNum")))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(ProductDetail {
        name,
        brand,
        price,
        original_price: None,
        currency: currency.to_string(),
        rating: None,
        review_count: None,
        product_url: format!("{}/pr/p/{}", base_url, product_id),
        product_id: product_id.to_string(),
        in_stock: true,
        description: None,
        product_code,
        upc: None,
        ingredients: None,
        supplement_facts: None,
        suggested_use: None,
        warnings: None,
        shipping_weight: None,
        category_breadcrumb: None,
        review_distribution: None,
    })
}

/// Enrich a ProductDetail with fields only available in the DOM (ingredients, supplement facts, etc.)
fn enrich_from_html(html: &str, product: &mut ProductDetail) {
    let doc = Html::parse_document(html);

    // Brand - if not already set, try DOM
    if product.brand.is_empty() {
        if let Some(brand) = extract_text(
            &doc,
            "#brand a span bdi, #brand a[data-testid='product-brand-link'] span bdi",
        ) {
            product.brand = brand;
        }
    }

    // Original price from the share-email hidden input
    if product.original_price.is_none() {
        if let Ok(sel) = Selector::parse("input#share-email-model") {
            if let Some(el) = doc.select(&sel).next() {
                let list_price = el
                    .value()
                    .attr("data-list-price")
                    .and_then(|s| parse_price_str(s));
                let disc_price = el
                    .value()
                    .attr("data-discount-price")
                    .and_then(|s| parse_price_str(s));
                if let (Some(list), Some(disc)) = (list_price, disc_price) {
                    if list > disc {
                        product.original_price = Some(list);
                        // Update price to discount price if we got it from JSON-LD as list price
                        if (product.price - list).abs() < 0.01 {
                            product.price = disc;
                        }
                    }
                }
            }
        }
    }

    // Rating from star link title (e.g., "4.8/5 - 42,328 Reviews")
    if product.rating.is_none() {
        if let Ok(sel) = Selector::parse("a.stars.scroll-to, a.stars") {
            if let Some(el) = doc.select(&sel).next() {
                if let Some(title) = el.value().attr("title") {
                    if let Some(rating) = title
                        .split('/')
                        .next()
                        .and_then(|s| s.trim().parse::<f64>().ok())
                    {
                        product.rating = Some(rating);
                    }
                }
            }
        }
    }

    // Review count
    if product.review_count.is_none() {
        if let Some(text) = extract_text(&doc, "a.rating-count span") {
            let num: String = text
                .replace(',', "")
                .chars()
                .filter(|c| c.is_ascii_digit())
                .collect();
            product.review_count = num.parse::<u32>().ok();
        }
    }

    // Availability
    if let Some(stock_text) = extract_text(&doc, "#stock-status .stock-status-content strong") {
        product.in_stock = stock_text.to_lowercase().contains("in stock");
    }

    // Shipping weight from product specs
    if product.shipping_weight.is_none() {
        product.shipping_weight = extract_spec(&doc, "Shipping Weight");
    }

    // Product code from specs if not set
    if product.product_code.is_none() {
        product.product_code = extract_spec(&doc, "Product Code");
    }

    // UPC from specs if not set
    if product.upc.is_none() {
        product.upc = extract_spec(&doc, "UPC");
    }

    // Parse the product overview section for structured content
    parse_overview_sections(html, product);

    // Supplement facts
    if product.supplement_facts.is_none() {
        product.supplement_facts = parse_supplement_facts_html(&doc);
    }

    // Review distribution from DOM
    if product.review_distribution.is_none() {
        product.review_distribution = parse_review_distribution_html(&doc);
    }
}

/// Parse structured sections (Suggested Use, Warnings, Ingredients, Description) from product overview.
fn parse_overview_sections(html: &str, product: &mut ProductDetail) {
    let doc = Html::parse_document(html);

    // Ingredients from .prodOverviewIngred
    if product.ingredients.is_none() {
        if let Some(text) = extract_text(&doc, ".prodOverviewIngred") {
            product.ingredients = Some(text);
        }
    }

    // Parse sections by heading text in product overview
    // Each section is: <h3><strong>Section Title</strong></h3> followed by <div class="prodOverviewDetail">content</div>
    if let Ok(h3_sel) = Selector::parse("#product-overview h3") {
        for h3 in doc.select(&h3_sel) {
            let heading_text: String = h3.text().collect::<Vec<_>>().join("").trim().to_lowercase();

            // Get the next sibling div content
            let mut next = h3.next_sibling();
            while let Some(node) = next {
                if let Some(el) = node.value().as_element() {
                    if el.name() == "div" {
                        let inner_html: String = node
                            .children()
                            .filter_map(|child| {
                                if let Some(text) = child.value().as_text() {
                                    Some(text.to_string())
                                } else if child.value().is_element() {
                                    let el_ref = scraper::ElementRef::wrap(child)?;
                                    Some(el_ref.text().collect::<Vec<_>>().join(" "))
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join(" ")
                            .trim()
                            .to_string();

                        if !inner_html.is_empty() {
                            if heading_text.contains("suggested use")
                                && product.suggested_use.is_none()
                            {
                                product.suggested_use = Some(inner_html);
                            } else if heading_text.contains("warning") && product.warnings.is_none()
                            {
                                product.warnings = Some(inner_html);
                            } else if heading_text.contains("description")
                                && product.description.is_none()
                            {
                                product.description = Some(inner_html);
                            }
                        }
                        break;
                    }
                }
                next = node.next_sibling();
            }
        }
    }
}

/// Extract a value from #product-specs-list by label prefix.
fn extract_spec(doc: &Html, label: &str) -> Option<String> {
    if let Ok(sel) = Selector::parse("#product-specs-list li") {
        for li in doc.select(&sel) {
            let text: String = li.text().collect::<Vec<_>>().join("").trim().to_string();
            if text.starts_with(label) {
                // Extract the value after the label and colon
                let value = text
                    .splitn(2, ':')
                    .nth(1)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                if value.is_some() {
                    return value;
                }
                // Try extracting from span child
                if let Ok(span_sel) = Selector::parse("span") {
                    if let Some(span) = li.select(&span_sel).next() {
                        let span_text: String =
                            span.text().collect::<Vec<_>>().join("").trim().to_string();
                        if !span_text.is_empty() {
                            return Some(span_text);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Parse product detail from __NEXT_DATA__ JSON (kept as fallback).
pub fn parse_from_next_data(
    data: &serde_json::Value,
    product_id: &str,
    base_url: &str,
) -> Option<ProductDetail> {
    let props = data.get("props")?.get("pageProps")?;

    let product = props
        .get("product")
        .or_else(|| props.get("productData"))
        .or_else(|| props.get("initialProduct"))?;

    let name = product
        .get("title")
        .or_else(|| product.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if name.is_empty() {
        return None;
    }

    let brand = product
        .get("brandName")
        .or_else(|| product.get("brand").and_then(|b| b.get("name")))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let price = product
        .get("price")
        .or_else(|| product.get("discountPrice"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    let original_price = product
        .get("listPrice")
        .or_else(|| product.get("retailPrice"))
        .and_then(|v| v.as_f64())
        .filter(|&p| p > price);

    let currency = product
        .get("currency")
        .and_then(|v| v.as_str())
        .unwrap_or("USD")
        .to_string();

    let rating = product
        .get("rating")
        .or_else(|| product.get("averageRating"))
        .and_then(|v| v.as_f64());

    let review_count = product
        .get("reviewCount")
        .or_else(|| product.get("numberOfReviews"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);

    let in_stock = product
        .get("inStock")
        .or_else(|| product.get("isInStock"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let description = product
        .get("description")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let product_code = product
        .get("partNumber")
        .or_else(|| product.get("productCode"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let upc = product
        .get("upc")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let ingredients = product
        .get("ingredients")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let suggested_use = product
        .get("suggestedUse")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let warnings = product
        .get("warnings")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let shipping_weight = product
        .get("shippingWeight")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let product_url = format!("{}/pr/p/{}", base_url, product_id);

    Some(ProductDetail {
        name,
        brand,
        price,
        original_price,
        currency,
        rating,
        review_count,
        product_url,
        product_id: product_id.to_string(),
        in_stock,
        description,
        product_code,
        upc,
        ingredients,
        supplement_facts: None,
        suggested_use,
        warnings,
        shipping_weight,
        category_breadcrumb: None,
        review_distribution: None,
    })
}

/// Fallback: Parse product detail from HTML using CSS selectors.
pub fn parse_from_html(
    html: &str,
    product_id: &str,
    base_url: &str,
    currency: &str,
) -> Result<ProductDetail, IherbError> {
    let doc = Html::parse_document(html);

    // Check for 404
    if html.contains("Page Not Found")
        || html.contains("<title>404</title>")
        || html.contains("404 Not Found")
    {
        return Err(IherbError::ProductNotFound(product_id.to_string()));
    }

    let name = extract_text(&doc, "h1#name, h1[data-testid='product-name'], h1")
        .unwrap_or_else(|| "Unknown Product".to_string());

    let brand = extract_text(
        &doc,
        "#brand a span bdi, #brand a[data-testid='product-brand-link'] span bdi",
    )
    .unwrap_or_default();

    // Price from share-email hidden input (most reliable)
    let (price, original_price) = extract_prices_from_input(&doc).unwrap_or_else(|| {
        let p = extract_text(
            &doc,
            ".purchase-option-one-time .list-price, #product-price .list-price, .price",
        )
        .and_then(|s| parse_price_str(&s))
        .unwrap_or(0.0);
        (p, None)
    });

    // Rating from star title attribute
    let rating = extract_rating_from_stars(&doc);

    // Review count
    let review_count = extract_text(&doc, "a.rating-count span").and_then(|s| {
        s.replace(',', "")
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .ok()
    });

    // Availability
    let in_stock = extract_text(&doc, "#stock-status .stock-status-content strong")
        .map(|s| s.to_lowercase().contains("in stock"))
        .unwrap_or(!html.contains("Out of Stock"));

    let product_code = extract_spec(&doc, "Product Code");
    let upc = extract_spec(&doc, "UPC");
    let shipping_weight = extract_spec(&doc, "Shipping Weight");

    let supplement_facts = parse_supplement_facts_html(&doc);
    let review_distribution = parse_review_distribution_html(&doc);

    // Detect actual currency from the page, falling back to config currency
    let detected_currency =
        super::search::detect_currency_from_html(&doc).unwrap_or_else(|| currency.to_string());

    let product_url = format!("{}/pr/p/{}", base_url, product_id);

    let mut product = ProductDetail {
        name,
        brand,
        price,
        original_price,
        currency: detected_currency,
        rating,
        review_count,
        product_url,
        product_id: product_id.to_string(),
        in_stock,
        description: None,
        product_code,
        upc,
        ingredients: None,
        supplement_facts,
        suggested_use: None,
        warnings: None,
        shipping_weight,
        category_breadcrumb: None,
        review_distribution,
    };

    // Parse structured overview sections
    parse_overview_sections(html, &mut product);

    Ok(product)
}

fn extract_prices_from_input(doc: &Html) -> Option<(f64, Option<f64>)> {
    let sel = Selector::parse("input#share-email-model").ok()?;
    let el = doc.select(&sel).next()?;

    let list_price = el
        .value()
        .attr("data-list-price")
        .and_then(|s| parse_price_str(s));
    let disc_price = el
        .value()
        .attr("data-discount-price")
        .and_then(|s| parse_price_str(s));

    match (disc_price, list_price) {
        (Some(disc), Some(list)) if list > disc => Some((disc, Some(list))),
        (Some(disc), _) => Some((disc, None)),
        (None, Some(list)) => Some((list, None)),
        _ => None,
    }
}

fn extract_rating_from_stars(doc: &Html) -> Option<f64> {
    let sel = Selector::parse("a.stars.scroll-to, a.stars").ok()?;
    let el = doc.select(&sel).next()?;
    let title = el.value().attr("title")?;
    // Title format: "4.8/5 - 42,328 Reviews"
    title.split('/').next()?.trim().parse::<f64>().ok()
}

fn extract_text(doc: &Html, selectors: &str) -> Option<String> {
    for selector_str in selectors.split(',') {
        let selector_str = selector_str.trim();
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = doc.select(&selector).next() {
                let text: String = element
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

fn parse_price_str(s: &str) -> Option<f64> {
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    cleaned.parse().ok()
}

fn parse_supplement_facts_html(doc: &Html) -> Option<SupplementFacts> {
    let table_sel =
        Selector::parse(".supplement-facts-container table, table.supplement-facts-table").ok()?;
    let table = doc.select(&table_sel).next()?;

    let row_sel = Selector::parse("tr").ok()?;
    let cell_sel = Selector::parse("td, th").ok()?;

    let mut nutrients = Vec::new();
    let mut serving_size = None;
    let mut servings_per_container = None;

    for row in table.select(&row_sel) {
        let cells: Vec<String> = row
            .select(&cell_sel)
            .map(|c| c.text().collect::<Vec<_>>().join("").trim().to_string())
            .collect();

        // Check for serving size info in merged cells
        if cells.len() == 1 {
            let text = &cells[0];
            let lower = text.to_lowercase();
            if lower.contains("serving size") {
                serving_size = text.splitn(2, ':').nth(1).map(|s| s.trim().to_string());
            } else if lower.contains("servings per") {
                servings_per_container = text.splitn(2, ':').nth(1).map(|s| s.trim().to_string());
            }
            continue;
        }

        // Skip header rows
        if cells.len() >= 2 {
            let first_lower = cells[0].to_lowercase();
            if first_lower.contains("amount per")
                || first_lower.contains("% daily")
                || first_lower.contains("supplement")
                || first_lower.is_empty()
            {
                continue;
            }
            // Skip dagger footnotes
            if cells[0].starts_with('†') || cells[0].starts_with('*') {
                continue;
            }

            nutrients.push(Nutrient {
                name: cells[0].clone(),
                amount: cells.get(1).cloned().unwrap_or_default(),
                daily_value: cells.get(2).cloned().filter(|s| !s.is_empty()),
            });
        }
    }

    if nutrients.is_empty() && serving_size.is_none() {
        return None;
    }

    Some(SupplementFacts {
        serving_size,
        servings_per_container,
        nutrients,
    })
}

fn parse_review_distribution_html(doc: &Html) -> Option<ReviewDistribution> {
    // Try to find review summary section
    let sel = Selector::parse("#product-reviews .review-summary, .rating-distribution, [data-testid='rating-distribution']").ok()?;
    let _el = doc.select(&sel).next()?;

    fn get_star_pct(doc: &Html, star: u8) -> Option<f64> {
        let sel_str = format!(
            "[data-testid='star-{}-pct'], .star-{}-pct, .star-percent-{}",
            star, star, star
        );
        if let Ok(sel) = Selector::parse(&sel_str) {
            if let Some(el) = doc.select(&sel).next() {
                let text: String = el.text().collect::<Vec<_>>().join("");
                return text.replace('%', "").trim().parse::<f64>().ok();
            }
        }
        None
    }

    Some(ReviewDistribution {
        five_star: get_star_pct(doc, 5),
        four_star: get_star_pct(doc, 4),
        three_star: get_star_pct(doc, 3),
        two_star: get_star_pct(doc, 2),
        one_star: get_star_pct(doc, 1),
    })
}
