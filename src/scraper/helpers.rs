use scraper::{Html, Selector};

/// Parse a price string by extracting digits, periods, and commas, then
/// determine the decimal separator based on position and context.
/// Handles both US format (1,234.56) and European format (1.234,56).
pub fn parse_price_str(s: &str) -> Option<f64> {
    // Keep only digits, periods, and commas
    let cleaned: String = s
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();

    if cleaned.is_empty() {
        return None;
    }

    let has_dot = cleaned.contains('.');
    let has_comma = cleaned.contains(',');

    let normalized = if has_dot && has_comma {
        // Both present: the LAST one is the decimal separator
        let last_dot = cleaned.rfind('.').unwrap();
        let last_comma = cleaned.rfind(',').unwrap();
        if last_comma > last_dot {
            // Comma is decimal (European: 1.234,56)
            cleaned.replace('.', "").replacen(',', ".", 1)
        } else {
            // Dot is decimal (US: 1,234.56)
            cleaned.replace(',', "")
        }
    } else if has_comma {
        // Only commas: check if it looks like a thousands separator
        let last_comma = cleaned.rfind(',').unwrap();
        let after_comma = &cleaned[last_comma + 1..];
        if after_comma.len() == 3 && after_comma.chars().all(|c| c.is_ascii_digit()) {
            // Exactly 3 digits after last comma => thousands separator (e.g. "1,000")
            cleaned.replace(',', "")
        } else {
            // Otherwise treat comma as decimal (e.g. "23,99")
            cleaned.replacen(',', ".", 1)
        }
    } else {
        // Only dots or no separator at all: parse normally
        cleaned
    };

    normalized.parse().ok()
}

/// Extract text from a document by trying comma-separated CSS selectors.
pub fn extract_text(doc: &Html, selectors: &str) -> Option<String> {
    for sel_str in selectors.split(',') {
        if let Ok(sel) = Selector::parse(sel_str.trim()) {
            if let Some(element) = doc.select(&sel).next() {
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

/// Extract text from an element reference by trying comma-separated CSS selectors.
pub fn extract_element_text(el: &scraper::ElementRef, selectors: &str) -> Option<String> {
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

/// Parse a review count from text by extracting digits.
pub fn parse_review_count(text: &str) -> Option<u32> {
    text.replace(',', "")
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect::<String>()
        .parse::<u32>()
        .ok()
}

/// Dump HTML to /tmp for debugging when debug level is enabled.
pub fn debug_dump_html(html: &str, label: &str) {
    if tracing::enabled!(tracing::Level::DEBUG) {
        let safe_label = label.replace(' ', "_");
        let dump_path = format!("/tmp/iherb_{}.html", safe_label);
        let _ = std::fs::write(&dump_path, html);
        tracing::debug!("Dumped HTML to {}", dump_path);
    }
}

/// Check if HTML indicates a 404/not-found page.
pub fn is_not_found_page(html: &str) -> bool {
    html.contains("Page Not Found")
        || html.contains("<title>404</title>")
        || html.contains("404 Not Found")
}

/// Detect the actual currency from HTML via meta tags or price text.
pub fn detect_currency_from_html(doc: &Html) -> Option<String> {
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
