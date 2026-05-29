// src/utils.rs
use chrono::{Datelike, NaiveDate};
use std::error::Error;

pub fn amount_to_cents(amount: f64) -> i64 {
    (amount * 100.0).round() as i64
}

pub fn cents_to_amount(cents: i64) -> f64 {
    cents as f64 / 100.0
}

pub fn format_money(cents: i64) -> String {
    let is_negative = cents < 0;
    let abs_cents = cents.abs();
    let dollars = abs_cents / 100;
    let remaining_cents = abs_cents % 100;

    let dollars_str = dollars.to_string();
    let mut formatted_dollars = String::new();
    
    let mut count = 0;
    for c in dollars_str.chars().rev() {
        if count == 3 {
            formatted_dollars.push('.');
            count = 0;
        }
        formatted_dollars.push(c);
        count += 1;
    }
    
    let dollars_formatted: String = formatted_dollars.chars().rev().collect();
    
    let sign = if is_negative { "-" } else { "" };
    
    if remaining_cents == 0 {
        format!("{}${}", sign, dollars_formatted)
    } else {
        format!("{}${},{:02}", sign, dollars_formatted, remaining_cents)
    }
}

pub fn resolve_dates(
    from: Option<String>,
    to: Option<String>,
    month: Option<u32>,
    year: Option<i32>,
) -> std::result::Result<(String, String), Box<dyn Error>> {
    let now = chrono::Local::now().naive_local().date();
    let resolved_year = year.unwrap_or(now.year());

    if let Some(m) = month {
        if m < 1 || m > 12 {
            return Err("Month must be between 1 and 12.".into());
        }
        let start_date = NaiveDate::from_ymd_opt(resolved_year, m, 1)
            .ok_or("Invalid date.")?;
        
        let end_date = if m == 12 {
            NaiveDate::from_ymd_opt(resolved_year + 1, 1, 1)
        } else {
            NaiveDate::from_ymd_opt(resolved_year, m + 1, 1)
        }
        .ok_or("Invalid end date.")? - chrono::Duration::days(1);

        return Ok((
            start_date.format("%Y-%m-%d").to_string(),
            end_date.format("%Y-%m-%d").to_string(),
        ));
    }

    if let Some(y) = year {
        if month.is_none() {
            let start_date = NaiveDate::from_ymd_opt(y, 1, 1).ok_or("Invalid date.")?;
            let end_date = NaiveDate::from_ymd_opt(y, 12, 31).ok_or("Invalid date.")?;
            return Ok((
                start_date.format("%Y-%m-%d").to_string(),
                end_date.format("%Y-%m-%d").to_string(),
            ));
        }
    }

    let final_from = from.unwrap_or_else(|| "1970-01-01".to_string());
    let final_to = to.unwrap_or_else(|| now.format("%Y-%m-%d").to_string());
    Ok((final_from, final_to))
}
