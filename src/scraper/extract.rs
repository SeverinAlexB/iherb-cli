use crate::error::IherbError;
use chromiumoxide::Page;

/// Extract __NEXT_DATA__ JSON from the page via JS evaluation.
pub async fn extract_next_data(page: &Page) -> Result<Option<serde_json::Value>, IherbError> {
    let script = r#"
        (function() {
            var el = document.getElementById('__NEXT_DATA__');
            if (el) return el.textContent;
            return null;
        })()
    "#;
    evaluate_and_parse_json(page, script, "__NEXT_DATA__").await
}

/// Extract JSON-LD structured data from the page.
pub fn extract_json_ld(html: &str) -> Option<serde_json::Value> {
    let doc = scraper::Html::parse_document(html);
    let sel = scraper::Selector::parse(r#"script[type="application/ld+json"]"#).ok()?;

    for el in doc.select(&sel) {
        let text: String = el.text().collect();
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
            if parsed.get("@type").and_then(|v| v.as_str()) == Some("Product") {
                tracing::debug!("Found JSON-LD Product data");
                return Some(parsed);
            }
            if let Some(arr) = parsed.as_array() {
                for item in arr {
                    if item.get("@type").and_then(|v| v.as_str()) == Some("Product") {
                        tracing::debug!("Found JSON-LD Product data in array");
                        return Some(item.clone());
                    }
                }
            }
        }
    }
    tracing::debug!("No JSON-LD Product data found");
    None
}

/// Extract JS globals (window.PRODUCT_DETAILS, window.IHR_DL) from the page via JS evaluation.
pub async fn extract_js_globals(page: &Page) -> Result<Option<serde_json::Value>, IherbError> {
    let script = r#"
        (function() {
            var result = {};
            if (window.PRODUCT_DETAILS) result.productDetails = window.PRODUCT_DETAILS;
            if (window.IHR_DL && window.IHR_DL.product) result.ihrProduct = window.IHR_DL.product;
            return Object.keys(result).length > 0 ? JSON.stringify(result) : null;
        })()
    "#;
    evaluate_and_parse_json(page, script, "JS globals").await
}

/// Evaluate a JS script on the page and parse the result as JSON.
async fn evaluate_and_parse_json(
    page: &Page,
    script: &str,
    label: &str,
) -> Result<Option<serde_json::Value>, IherbError> {
    match page.evaluate(script).await {
        Ok(val) => {
            let text = val.into_value::<Option<String>>().unwrap_or(None);
            match text {
                Some(json_str) if !json_str.is_empty() => {
                    tracing::debug!("Found {} ({} bytes)", label, json_str.len());
                    match serde_json::from_str(&json_str) {
                        Ok(parsed) => Ok(Some(parsed)),
                        Err(e) => {
                            tracing::warn!("Failed to parse {}: {}", label, e);
                            Ok(None)
                        }
                    }
                }
                _ => {
                    tracing::debug!("No {} found", label);
                    Ok(None)
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to evaluate {} script: {}", label, e);
            Ok(None)
        }
    }
}
