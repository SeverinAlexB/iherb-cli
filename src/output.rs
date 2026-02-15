use crate::cli::Section;
use crate::model::{ProductDetail, SearchResult};

pub fn format_search_results(result: &SearchResult) -> String {
    let mut out = String::new();

    let total_str = match result.total_results {
        Some(total) => format!("{}+", format_number(total)),
        None => "?".to_string(),
    };
    let showing = result.products.len();
    out.push_str(&format!(
        "## Search results for \"{}\" (showing {} of {})\n\n",
        result.query, showing, total_str
    ));

    for (i, product) in result.products.iter().enumerate() {
        out.push_str(&format!("### {}. {}\n", i + 1, product.name));
        out.push_str(&format!("- **Brand:** {}\n", product.brand));

        let price_str = format_price(
            product.price,
            product.original_price.as_ref(),
            &product.currency,
        );
        out.push_str(&format!("- **Price:** {}\n", price_str));

        if let (Some(rating), Some(count)) = (product.rating, product.review_count) {
            out.push_str(&format!(
                "- **Rating:** {:.1}/5 ({} reviews)\n",
                rating,
                format_number(count)
            ));
        }

        out.push_str(&format!("- **ID:** {}\n", product.product_id));
        out.push_str(&format!("- **URL:** {}\n", product.product_url));

        if i < result.products.len() - 1 {
            out.push_str("\n---\n\n");
        }
    }

    out
}

pub fn format_product_detail(product: &ProductDetail, section: Option<Section>) -> String {
    let mut out = String::new();

    let sections: &[Section] = match section {
        Some(s) => &[s],
        None => Section::ALL,
    };

    if section.is_none() {
        out.push_str(&format!("# {}\n\n", product.name));
    }

    for sec in sections {
        match sec {
            Section::Overview => format_overview(product, &mut out),
            Section::Description => format_description(product, &mut out),
            Section::Nutrition => format_nutrition(product, &mut out),
            Section::Ingredients => format_ingredients(product, &mut out),
            Section::SuggestedUse => format_suggested_use(product, &mut out),
            Section::Warnings => format_warnings(product, &mut out),
            Section::Reviews => format_reviews(product, &mut out),
        }
    }

    out
}

fn format_overview(product: &ProductDetail, out: &mut String) {
    out.push_str("## Overview\n");
    out.push_str(&format!("- **Brand:** {}\n", product.brand));

    let price_str = format_price(
        product.price,
        product.original_price.as_ref(),
        &product.currency,
    );
    out.push_str(&format!("- **Price:** {}\n", price_str));

    if let (Some(rating), Some(count)) = (product.rating, product.review_count) {
        out.push_str(&format!(
            "- **Rating:** {:.1}/5 ({} reviews)\n",
            rating,
            format_number(count)
        ));
    }

    let stock_str = if product.in_stock {
        "In Stock"
    } else {
        "Out of Stock"
    };
    out.push_str(&format!("- **Availability:** {}\n", stock_str));

    if let Some(ref code) = product.product_code {
        out.push_str(&format!("- **Product Code:** {}\n", code));
    }
    if let Some(ref weight) = product.shipping_weight {
        out.push_str(&format!("- **Shipping Weight:** {}\n", weight));
    }
    out.push('\n');
}

fn format_description(product: &ProductDetail, out: &mut String) {
    if let Some(ref desc) = product.description {
        out.push_str("## Description\n");
        out.push_str(desc);
        out.push_str("\n\n");
    }
}

fn format_nutrition(product: &ProductDetail, out: &mut String) {
    let facts = match product.supplement_facts {
        Some(ref f) => f,
        None => return,
    };
    out.push_str("## Supplement Facts\n");
    if !facts.nutrients.is_empty() {
        out.push_str("| Nutrient | Amount | % Daily Value |\n");
        out.push_str("|---|---|---|\n");
        for nutrient in &facts.nutrients {
            let dv = nutrient.daily_value.as_deref().unwrap_or("");
            out.push_str(&format!(
                "| {} | {} | {} |\n",
                nutrient.name, nutrient.amount, dv
            ));
        }
        out.push('\n');
    }
    if let Some(ref size) = facts.serving_size {
        out.push_str(&format!("- **Serving Size:** {}\n", size));
    }
    if let Some(ref servings) = facts.servings_per_container {
        out.push_str(&format!("- **Servings Per Container:** {}\n", servings));
    }
    out.push('\n');
}

fn format_ingredients(product: &ProductDetail, out: &mut String) {
    if let Some(ref ingredients) = product.ingredients {
        out.push_str("## Other Ingredients\n");
        out.push_str(ingredients);
        out.push_str("\n\n");
    }
}

fn format_suggested_use(product: &ProductDetail, out: &mut String) {
    if let Some(ref usage) = product.suggested_use {
        out.push_str("## Suggested Use\n");
        out.push_str(usage);
        out.push_str("\n\n");
    }
}

fn format_warnings(product: &ProductDetail, out: &mut String) {
    if let Some(ref warnings) = product.warnings {
        out.push_str("## Warnings\n");
        out.push_str(warnings);
        out.push_str("\n\n");
    }
}

fn format_reviews(product: &ProductDetail, out: &mut String) {
    let dist = match product.review_distribution {
        Some(ref d) => d,
        None => return,
    };
    out.push_str("## Reviews\n");
    if let (Some(rating), Some(count)) = (product.rating, product.review_count) {
        out.push_str(&format!("- **Average:** {:.1}/5\n", rating));
        out.push_str(&format!("- **Total:** {} reviews\n", format_number(count)));
    }
    if let Some(pct) = dist.five_star {
        out.push_str(&format!("- 5 stars: {:.0}%\n", pct));
    }
    if let Some(pct) = dist.four_star {
        out.push_str(&format!("- 4 stars: {:.0}%\n", pct));
    }
    if let Some(pct) = dist.three_star {
        out.push_str(&format!("- 3 stars: {:.0}%\n", pct));
    }
    if let Some(pct) = dist.two_star {
        out.push_str(&format!("- 2 stars: {:.0}%\n", pct));
    }
    if let Some(pct) = dist.one_star {
        out.push_str(&format!("- 1 star: {:.0}%\n", pct));
    }
    out.push('\n');
}

fn format_price(price: f64, original: Option<&f64>, currency: &str) -> String {
    let symbol = match currency {
        "USD" => "$",
        "CHF" => "CHF ",
        "EUR" => "€",
        "GBP" => "£",
        _ => currency,
    };

    match original {
        Some(orig) if *orig > price => {
            let discount = ((*orig - price) / *orig * 100.0).round() as u32;
            format!(
                "{}{:.2} ~~{}{:.2}~~ ({}% off)",
                symbol, price, symbol, orig, discount
            )
        }
        _ => format!("{}{:.2}", symbol, price),
    }
}

fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}
