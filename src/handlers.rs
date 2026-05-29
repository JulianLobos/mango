// src/handlers.rs
use chrono::{Datelike, Local, Months, NaiveDate};
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Table};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::Write;
use uuid::Uuid;

use crate::cli::{AccountCommands, BudgetCommands};
use crate::db::*;
use crate::models::{EntryInput, TransactionInput};
use crate::utils::{amount_to_cents, cents_to_amount, format_money, resolve_dates};

pub fn handle_account(
    conn: &mut Connection,
    cmd: AccountCommands,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    match cmd {
        AccountCommands::Add {
            name,
            r#type,
            currency,
        } => handle_account_add(conn, name, r#type, currency),
        AccountCommands::List => handle_account_list(conn),
        AccountCommands::Update {
            old_name,
            new_name,
            currency,
        } => handle_account_update(conn, old_name, new_name, currency),
        AccountCommands::Delete { name } => handle_account_delete(conn, name),
        AccountCommands::Adjust { name, new_balance, description, exchange_rate } =>
            handle_account_adjust(conn, name, new_balance, description, exchange_rate),
    }
}

pub fn handle_account_add(
    conn: &Connection,
    name: String,
    r#type: String,
    currency: String,
) -> std::result::Result<(), Box<dyn Error>> {
    let valid_types = vec!["asset", "liability", "income", "expense", "equity"];
    if !valid_types.contains(&r#type.to_lowercase().as_str()) {
        return Err(format!(
            "Invalid account type '{}'. Must be one of: asset, liability, income, expense, equity.",
            r#type
        )
        .into());
    }

    let res = conn.execute(
        "INSERT INTO accounts (name, type, currency) VALUES (?, ?, ?);",
        params![name, r#type.to_lowercase(), currency.to_uppercase()],
    );

    match res {
        Ok(_) => {
            println!("Account '{}' ({}) added successfully in {}!", name, r#type, currency.to_uppercase());
            Ok(())
        }
        Err(e) => {
            if e.to_string().contains("UNIQUE constraint failed") {
                Err(format!("Account '{}' already exists.", name).into())
            } else {
                Err(e.into())
            }
        }
    }
}

pub fn handle_account_list(conn: &Connection) -> std::result::Result<(), Box<dyn Error>> {
    let mut stmt = conn.prepare(
        "SELECT name, type, currency FROM accounts ORDER BY type ASC, name ASC;",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Name").fg(Color::Cyan),
            Cell::new("Type").fg(Color::Cyan),
            Cell::new("Currency").fg(Color::Cyan),
        ]);

    for row in rows {
        let (name, ty, currency) = row?;
        table.add_row(vec![
            Cell::new(name),
            Cell::new(ty),
            Cell::new(currency),
        ]);
    }

    println!("
=== REGISTERED ACCOUNTS ===");
    println!("{}", table);
    Ok(())
}

pub fn handle_account_update(
    conn: &Connection,
    old_name: String,
    new_name: Option<String>,
    currency: Option<String>,
) -> std::result::Result<(), Box<dyn Error>> {
    if new_name.is_none() && currency.is_none() {
        return Err("Nothing to update. Provide --new-name or --currency.".into());
    }

    find_account_id(conn, &old_name)?;

    if let Some(ref name) = new_name {
        conn.execute(
            "UPDATE accounts SET name = ? WHERE name = ? COLLATE NOCASE;",
            params![name, old_name],
        )?;
        println!("Account '{}' renamed to '{}'.", old_name, name);
    }

    if let Some(ref curr) = currency {
        let target_name = new_name.as_ref().unwrap_or(&old_name);
        conn.execute(
            "UPDATE accounts SET currency = ? WHERE name = ? COLLATE NOCASE;",
            params![curr.to_uppercase(), target_name],
        )?;
        println!("Currency for account '{}' updated to {}.", target_name, curr.to_uppercase());
    }

    Ok(())
}

pub fn handle_account_delete(conn: &Connection, name: String) -> std::result::Result<(), Box<dyn Error>> {
    let account_id = find_account_id(conn, &name)?;

    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entries WHERE account_id = ?;",
        params![account_id],
        |row| row.get(0),
    )?;

    if count > 0 {
        return Err(format!(
            "Cannot delete account '{}' because it has {} existing transactions. Delete the transactions first.",
            name, count
        )
        .into());
    }

    conn.execute("DELETE FROM accounts WHERE id = ?;", params![account_id])?;
    println!("Account '{}' deleted successfully.", name);
    Ok(())
}

pub fn handle_account_adjust(
    conn: &mut Connection,
    name: String,
    new_balance: f64,
    description: Option<String>,
    exchange_rate: Option<f64>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    // 1. Get account info and validate
    let account_id = find_account_id(conn, &name)?;
    let account_type: String = conn.query_row("SELECT type FROM accounts WHERE id = ?", params![account_id], |r| r.get(0))?;
    if account_type != "asset" {
        return Err(format!("Account '{}' is not an asset account. Adjustment only works for assets.", name).into());
    }

    // 2. Calculate current balance
    let current_balance_cents: i64 = conn.query_row(
        "SELECT COALESCE(SUM(amount_native), 0) FROM entries WHERE account_id = ?;",
        params![account_id],
        |row| row.get(0),
    )?;

    // 3. Calculate adjustment
    let new_balance_cents = amount_to_cents(new_balance);
    let adjustment_native_cents = new_balance_cents - current_balance_cents;

    if adjustment_native_cents == 0 {
        println!("Account '{}' balance is already {}. No adjustment needed.", name, format_money(new_balance_cents));
        return Ok(());
    }

    // 4. Ensure adjustment equity account exists and prepare entries
    let base_currency = get_base_currency(conn)?;
    let adj_account_currency = get_account_currency(conn, &name)?;
    let adjustment_account_name = "Equity Adjustments".to_string();

    if find_account_id(conn, &adjustment_account_name).is_err() {
        println!("Creating equity account '{}' for balance adjustments.", adjustment_account_name);
        handle_account_add(conn, adjustment_account_name.clone(), "equity".to_string(), base_currency.clone())?;
    }

    let mut entries = Vec::new();

    if adj_account_currency != base_currency {
        let rate = exchange_rate.ok_or_else(|| "Error: --exchange-rate is required to adjust a non-base currency account.")?;
        let adjustment_base_cents = (adjustment_native_cents as f64 * rate).round() as i64;
        
        entries.push(EntryInput {
            account_name: name.clone(),
            amount_native: adjustment_native_cents,
            exchange_rate: Some(rate),
        });
        entries.push(EntryInput {
            account_name: adjustment_account_name.clone(),
            amount_native: -adjustment_base_cents,
            exchange_rate: None, // This account is in base currency
        });
    } else {
        if exchange_rate.is_some() {
            println!("Warning: --exchange-rate is ignored when adjusting a base currency account.");
        }
        entries.push(EntryInput {
            account_name: name.clone(),
            amount_native: adjustment_native_cents,
            exchange_rate: None,
        });
        entries.push(EntryInput {
            account_name: adjustment_account_name.clone(),
            amount_native: -adjustment_native_cents,
            exchange_rate: None,
        });
    }
    
    // 5. Create transaction
    let tx_description = description.unwrap_or_else(|| format!("Balance adjustment for account '{}'", name));
    let tx_date = Local::now().naive_local().format("%Y-%m-%d").to_string();

    let tx_input = TransactionInput {
        date: tx_date,
        description: tx_description,
        installment_current: None,
        installment_total: None,
        parent_group_id: None,
        entries,
    };

    let tx_id = insert_double_entry_transaction(conn, tx_input)?;

    println!("Successfully adjusted balance for '{}'.", name);
    println!("  Old Balance: {}", format_money(current_balance_cents));
    println!("  New Balance: {}", format_money(new_balance_cents));
    println!("  Adjustment Transaction ID: {}", tx_id);

    Ok(())
}

pub fn handle_budget(
    conn: &mut Connection,
    cmd: BudgetCommands,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    match cmd {
        BudgetCommands::Set { category, limit } => handle_budget_set(conn, category, limit),
        BudgetCommands::List => handle_budget_list(conn),
        BudgetCommands::Delete { category } => handle_budget_delete(conn, category),
    }
}

pub fn handle_budget_set(
    conn: &Connection,
    category: String,
    limit: f64,
) -> std::result::Result<(), Box<dyn Error>> {
    let account_id = find_account_id(conn, &category)?;
    
    let ty: String = conn.query_row(
        "SELECT type FROM accounts WHERE id = ?;",
        params![account_id],
        |row| row.get(0),
    )?;

    if ty != "expense" {
        return Err(format!("Budget can only be set for 'expense' categories. '{}' is an {}.", category, ty).into());
    }

    let limit_cents = amount_to_cents(limit);
    if limit_cents <= 0 {
        return Err("Budget limit must be greater than zero.".into());
    }

    conn.execute(
        "INSERT OR REPLACE INTO budgets (account_id, amount_limit) VALUES (?, ?);",
        params![account_id, limit_cents],
    )?;

    println!("Budget limit of ${:.2} set for category '{}'.", limit, category);
    Ok(())
}

pub fn handle_budget_list(conn: &Connection) -> std::result::Result<(), Box<dyn Error>> {
    let mut stmt = conn.prepare(
        "SELECT a.name, b.amount_limit, a.currency 
         FROM budgets b
         JOIN accounts a ON b.account_id = a.id
         ORDER BY a.name ASC;",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Monthly Limit").fg(Color::Cyan),
            Cell::new("Currency").fg(Color::Cyan),
        ]);

    let mut count = 0;
    let mut total_cents_by_currency: HashMap<String, i64> = HashMap::new();

    for row in rows {
        let (name, limit_cents, currency) = row?;
        
        let total = total_cents_by_currency.entry(currency.clone()).or_insert(0);
        *total += limit_cents;

        table.add_row(vec![
            Cell::new(name),
            Cell::new(format_money(limit_cents)),
            Cell::new(currency),
        ]);
        count += 1;
    }

    if count == 0 {
        println!("No active budget limits found.");
        return Ok(());
    }

    // Add total rows
    for (currency, total_cents) in total_cents_by_currency {
        table.add_row(vec![
            Cell::new("TOTAL").fg(Color::Yellow),
            Cell::new(format_money(total_cents)).fg(Color::Yellow),
            Cell::new(currency).fg(Color::Yellow),
        ]);
    }

    println!("
=== ACTIVE MONTHLY BUDGETS ===");
    println!("{}", table);
    Ok(())
}

pub fn handle_budget_delete(conn: &Connection, category: String) -> std::result::Result<(), Box<dyn Error>> {
    let account_id = find_account_id(conn, &category)?;
    
    let affected = conn.execute(
        "DELETE FROM budgets WHERE account_id = ?;",
        params![account_id],
    )?;

    if affected == 0 {
        println!("No budget limit was set for category '{}'.", category);
    } else {
        println!("Budget limit for category '{}' has been removed.", category);
    }

    Ok(())
}

pub fn handle_pay_liability(
    conn: &mut Connection,
    liability_account: String,
    asset_account: String,
    month: u32,
    year: i32,
    date: Option<String>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let liability_id = find_account_id(conn, &liability_account)?;
    let liability_type: String = conn.query_row("SELECT type FROM accounts WHERE id = ?", params![liability_id], |r| r.get(0))?;
    if liability_type != "liability" {
         return Err(format!("Account '{}' is not a liability account.", liability_account).into());
    }

    let asset_id = find_account_id(conn, &asset_account)?;
    let asset_type: String = conn.query_row("SELECT type FROM accounts WHERE id = ?", params![asset_id], |r| r.get(0))?;
    if asset_type != "asset" {
        return Err(format!("Account '{}' is not an asset account.", asset_account).into());
    }

    let liability_currency = get_account_currency(conn, &liability_account)?;
    let asset_currency = get_account_currency(conn, &asset_account)?;

    if liability_currency != asset_currency {
         return Err("Cross-currency payments are not yet supported by pay-liability. Please use 'mango transfer' with an exchange rate.".into());
    }

    let (start_date, end_date) = resolve_dates(None, None, Some(month), Some(year))?;
    
    let debt_cents: i64 = conn.query_row(
        "SELECT COALESCE(SUM(e.amount_native), 0) FROM entries e
         JOIN transactions t ON e.transaction_id = t.id
         WHERE e.account_id = ?1 AND e.amount_native < 0 
         AND t.date >= ?2 AND t.date <= ?3",
        params![liability_id, start_date, end_date],
        |row| row.get(0),
    )?;

    if debt_cents == 0 {
        println!("No debt found for '{}' in {}/{}. Nothing to pay.", liability_account, month, year);
        return Ok(());
    }

    let payment_cents = -debt_cents;

    println!("Total debt calculated for '{}' in {}/{}: {}", liability_account, month, year, format_money(debt_cents));
    println!("Creating payment transfer of {} from '{}'.", format_money(payment_cents), asset_account);

    let payment_date = date.unwrap_or_else(|| Local::now().naive_local().format("%Y-%m-%d").to_string());
    let description = format!("Payment for {} statement {}/{}", liability_account, month, year);

    let tx_input = TransactionInput {
        date: payment_date,
        description,
        installment_current: None,
        installment_total: None,
        parent_group_id: None,
        entries: vec![
            EntryInput {
                account_name: asset_account.clone(),
                amount_native: -payment_cents,
                exchange_rate: None,
            },
            EntryInput {
                account_name: liability_account.clone(),
                amount_native: payment_cents,
                exchange_rate: None,
            },
        ],
    };

    let tx_id = insert_double_entry_transaction(conn, tx_input)?;

    println!("Successfully recorded payment transaction with ID: {}.", tx_id);

    Ok(())
}

pub fn handle_list(
    conn: &Connection,
    limit: u32,
    month: Option<u32>,
    year: Option<i32>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let mut query = "SELECT id, date, description FROM transactions".to_string();
    let title;

    let transactions = if month.is_some() || year.is_some() {
        let (start_date, end_date) = resolve_dates(None, None, month, year)?;
        
        query.push_str(" WHERE date >= ?1 AND date <= ?2");
        query.push_str(" ORDER BY date DESC, id DESC;");
        
        let mut title_parts = Vec::new();
        if let Some(m) = month {
            title_parts.push(format!("Month: {}", m));
        }
        let now = chrono::Local::now().naive_local().date();
        let resolved_year = year.unwrap_or_else(|| now.year());

        if year.is_some() || month.is_some() {
             if !title_parts.is_empty() { title_parts.push(", ".to_string()); }
             title_parts.push(format!("Year: {}", resolved_year));
        }
        title = format!("
=== TRANSACTIONS ({}) ===", title_parts.join(""));
        
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params![start_date, end_date], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    } else {
        query.push_str(" ORDER BY date DESC, id DESC LIMIT ?1;");
        title = format!("
=== RECENT TRANSACTIONS (Last {}) ===", limit);
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };

    if transactions.is_empty() {
        println!("No transactions found for the specified criteria.");
        return Ok(());
    }

    println!("{}", title);
    format_transaction_table(conn, transactions)?;

    Ok(())
}

pub fn handle_find(conn: &Connection, query: String) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let mut stmt = conn.prepare(
        "SELECT id, date, description FROM transactions WHERE description LIKE ? ORDER BY date DESC, id DESC;",
    )?;

    let search_pattern = format!("%{}%", query);
    let tx_rows = stmt.query_map(params![search_pattern], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    let mut transactions = Vec::new();
    for row in tx_rows {
        transactions.push(row?);
    }

    if transactions.is_empty() {
        println!("No transactions matching '{}' were found.", query);
        return Ok(());
    }

    println!("
=== SEARCH RESULTS FOR '{}' ===", query);
    format_transaction_table(conn, transactions)?;

    Ok(())
}

pub fn format_transaction_table(
    conn: &Connection,
    transactions: Vec<(i64, String, String)>,
) -> std::result::Result<(), Box<dyn Error>> {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("ID").fg(Color::Cyan),
            Cell::new("Date").fg(Color::Cyan),
            Cell::new("Description").fg(Color::Cyan),
            Cell::new("Details (Account: Native Amount)").fg(Color::Cyan),
        ]);

    for (tx_id, date, desc) in transactions {
        let mut entry_stmt = conn.prepare(
            "SELECT a.name, e.amount_native, a.currency 
             FROM entries e 
             JOIN accounts a ON e.account_id = a.id 
             WHERE e.transaction_id = ?;",
        )?;

        let entry_rows = entry_stmt.query_map(params![tx_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        let mut details = Vec::new();
        for entry in entry_rows {
            let (acc_name, cents, curr) = entry?;
            let amount = format_money(cents);
            details.push(format!("{}: {} {}", acc_name, amount, curr));
        }

        table.add_row(vec![
            Cell::new(tx_id.to_string()),
            Cell::new(date),
            Cell::new(desc),
            Cell::new(details.join("
")),
        ]);
    }

    println!("{}", table);
    Ok(())
}

pub fn handle_setup(conn: &Connection, base_currency: String) -> std::result::Result<(), Box<dyn Error>> {
    println!("Initializing SQLite database schema for Base Currency Accounting...");
    create_schema(conn)?;
    
    conn.execute(
        "INSERT OR IGNORE INTO settings (key, value) VALUES ('base_currency', ?);",
        params![base_currency.to_uppercase()],
    )?;
    
    println!("Database schema initialized successfully!");
    println!("Base Currency set to: {}", base_currency.to_uppercase());
    println!("You can now add your accounts using 'mango account add'.");
    Ok(())
}

pub fn handle_transfer(
    conn: &mut Connection,
    amount: f64,
    from: String,
    to: String,
    description: String,
    date: Option<String>,
    exchange_rate: Option<f64>,
    cost_amount: Option<f64>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let amount_native_cents = amount_to_cents(amount);
    if amount_native_cents <= 0 {
        return Err("Transfer amount must be greater than zero.".into());
    }

    let tx_date = match date {
        Some(d) => {
            if NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_err() {
                return Err("Invalid date format. Use YYYY-MM-DD.".into());
            }
            d
        }
        None => chrono::Local::now().naive_local().format("%Y-%m-%d").to_string(),
    };

    let base_currency = get_base_currency(conn)?;
    let from_currency = get_account_currency(conn, &from)?;
    let to_currency = get_account_currency(conn, &to)?;

    let mut from_rate = 1.0;
    let mut to_rate = 1.0;
    let from_native_cents = -amount_native_cents;
    let mut to_native_cents = amount_native_cents;

    if from_currency != to_currency {
        if let Some(cost) = cost_amount {
            let cost_cents = amount_to_cents(cost);
            to_native_cents = cost_cents;

            if from_currency == base_currency {
                from_rate = 1.0;
                to_rate = amount_native_cents as f64 / cost_cents as f64;
            } else if to_currency == base_currency {
                to_rate = 1.0;
                from_rate = cost_cents as f64 / amount_native_cents as f64;
            } else {
                return Err("For cross-currency transfers, at least one account must be in the Base Currency.".into());
            }
        } else if let Some(rate) = exchange_rate {
            if from_currency == base_currency {
                from_rate = 1.0;
                to_native_cents = (amount_native_cents as f64 / rate).round() as i64;
                to_rate = amount_native_cents as f64 / to_native_cents as f64;
            } else if to_currency == base_currency {
                to_rate = 1.0;
                to_native_cents = (amount_native_cents as f64 * rate).round() as i64;
                from_rate = to_native_cents as f64 / amount_native_cents as f64;
            } else {
                return Err("For cross-currency transfers, at least one account must be in the Base Currency.".into());
            }
        } else {
            return Err("Cross-currency transfer detected! You must provide --exchange-rate or --cost-amount.".into());
        }
    }

    let tx_input = TransactionInput {
        date: tx_date,
        description,
        installment_current: None,
        installment_total: None,
        parent_group_id: None,
        entries: vec![
            EntryInput {
                account_name: from.clone(),
                amount_native: from_native_cents,
                exchange_rate: Some(from_rate),
            },
            EntryInput {
                account_name: to.clone(),
                amount_native: to_native_cents,
                exchange_rate: Some(to_rate),
            },
        ],
    };

    let tx_id = insert_double_entry_transaction(conn, tx_input)?;
    println!("Transfer recorded successfully! Transaction ID: {}", tx_id);

    println!("  - {}: -{}", from, format_money(amount_native_cents));
    println!("  - {}: +{}", to, format_money(to_native_cents));

    Ok(())
}

pub fn handle_delete(conn: &Connection, id: i64) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let affected = conn.execute("DELETE FROM transactions WHERE id = ?;", params![id])?;

    if affected == 0 {
        return Err(format!("Transaction with ID {} not found.", id).into());
    }

    println!("Transaction {} and all its entries deleted successfully.", id);
    Ok(())
}

pub fn handle_edit(
    conn: &mut Connection,
    id: i64,
    description: Option<String>,
    amount: Option<f64>,
    date: Option<String>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    if description.is_none() && amount.is_none() && date.is_none() {
        return Err("Nothing to edit. Provide --description, --amount, or --date.".into());
    }

    let tx_exists: bool = conn.query_row(
        "SELECT 1 FROM transactions WHERE id = ?;",
        params![id],
        |_| Ok(true),
    ).unwrap_or(false);

    if !tx_exists {
        return Err(format!("Transaction ID {} not found.", id).into());
    }

    let sql_tx = conn.transaction()?;

    if let Some(desc) = description {
        sql_tx.execute("UPDATE transactions SET description = ? WHERE id = ?;", params![desc, id])?;
        println!("Description updated for transaction {}.", id);
    }

    if let Some(d) = date {
        if NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_err() {
            return Err("Invalid date format. Use YYYY-MM-DD.".into());
        }
        sql_tx.execute("UPDATE transactions SET date = ? WHERE id = ?;", params![d, id])?;
        println!("Date updated for transaction {}.", id);
    }

    if let Some(new_val) = amount {
        let new_cents = amount_to_cents(new_val);
        if new_cents <= 0 {
            return Err("Amount must be greater than zero.".into());
        }

        let entry_count: i64 = sql_tx.query_row(
            "SELECT COUNT(*) FROM entries WHERE transaction_id = ?;",
            params![id],
            |row| row.get(0),
        )?;

        if entry_count != 2 {
            return Err("Cannot edit amount of complex transactions (more than 2 entries). Please delete and re-record.".into());
        }

        let mut stmt = sql_tx.prepare("SELECT id, amount_native, exchange_rate FROM entries WHERE transaction_id = ?;")?;
        let entry_data: Vec<(i64, i64, f64)> = stmt.query_map(params![id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?.collect::<Result<Vec<_>, _>>()?;

        for (entry_id, old_native_cents, rate) in entry_data {
            let sign = if old_native_cents >= 0 { 1 } else { -1 };
            let updated_native_cents = new_cents * sign;
            let updated_base_cents = (updated_native_cents as f64 * rate).round() as i64;
            
            sql_tx.execute(
                "UPDATE entries SET amount_native = ?, amount_base = ? WHERE id = ?;", 
                params![updated_native_cents, updated_base_cents, entry_id]
            )?;
        }

        println!("Amount updated for transaction {}.", id);
    }

    sql_tx.commit()?;
    Ok(())
}

pub fn handle_export(conn: &Connection, file_path: &str) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let mut stmt = conn.prepare(
        "SELECT t.id, t.date, t.description, a.name, a.type, e.amount_native, e.amount_base, e.exchange_rate, a.currency
         FROM transactions t
         JOIN entries e ON t.id = e.transaction_id
         JOIN accounts a ON e.account_id = a.id
         ORDER BY t.date ASC, t.id ASC;",
    )?;

    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, i64>(6)?,
            row.get::<_, f64>(7)?,
            row.get::<_, String>(8)?,
        ))
    })?;

    let mut file = File::create(file_path)?;
    
    file.write_all(b"\xEF\xBB\xBF")?;

    writeln!(file, "ID,Date,Description,Account,Type,Native Amount,Base Amount,Exchange Rate,Currency")?;

    let mut count = 0;
    for row in rows {
        let (id, date, desc, acc, ty, native_cents, base_cents, rate, curr) = row?;
        
        let native_amount = native_cents as f64 / 100.0;
        let base_amount = base_cents as f64 / 100.0;
        
        let safe_desc = if desc.contains(',') {
            format!("\"{}\"", desc)
        } else {
            desc
        };

        writeln!(
            file,
            "{},{},{},{},{},{:.2},{:.2},{:.4},{}",
            id, date, safe_desc, acc, ty, native_amount, base_amount, rate, curr
        )?;
        count += 1;
    }

    println!("Exported {} entries to '{}' successfully.", count, file_path);
    Ok(())
}

pub fn handle_expense(
    conn: &mut Connection,
    amount: f64,
    account: String,
    category: String,
    description: String,
    date: Option<String>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let amount_cents = amount_to_cents(amount);
    if amount_cents <= 0 {
        return Err("Expense amount must be greater than zero.".into());
    }

    let tx_date = match date {
        Some(d) => {
            if NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_err() {
                return Err("Invalid date format. Use YYYY-MM-DD.".into());
            }
            d
        }
        None => chrono::Local::now().naive_local().format("%Y-%m-%d").to_string(),
    };

    let tx_input = TransactionInput {
        date: tx_date,
        description,
        installment_current: None,
        installment_total: None,
        parent_group_id: None,
        entries: vec![
            EntryInput {
                account_name: account.clone(),
                amount_native: -amount_cents,
                exchange_rate: None,
            },
            EntryInput {
                account_name: category.clone(),
                amount_native: amount_cents,
                exchange_rate: None,
            },
        ],
    };

    let tx_id = insert_double_entry_transaction(conn, tx_input)?;
    println!("Expense recorded successfully! Transaction ID: {}", tx_id);
    println!("  - {}: -{}", account, format_money(amount_cents));
    println!("  - {}: +{}", category, format_money(amount_cents));

    Ok(())
}

pub fn handle_income(
    conn: &mut Connection,
    amount: f64,
    account: String,
    source: String,
    description: String,
    date: Option<String>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let amount_cents = amount_to_cents(amount);
    if amount_cents <= 0 {
        return Err("Income amount must be greater than zero.".into());
    }

    let tx_date = match date {
        Some(d) => {
            if NaiveDate::parse_from_str(&d, "%Y-%m-%d").is_err() {
                return Err("Invalid date format. Use YYYY-MM-DD.".into());
            }
            d
        }
        None => chrono::Local::now().naive_local().format("%Y-%m-%d").to_string(),
    };

    let tx_input = TransactionInput {
        date: tx_date,
        description,
        installment_current: None,
        installment_total: None,
        parent_group_id: None,
        entries: vec![
            EntryInput {
                account_name: account.clone(),
                amount_native: amount_cents,
                exchange_rate: None,
            },
            EntryInput {
                account_name: source.clone(),
                amount_native: -amount_cents,
                exchange_rate: None,
            },
        ],
    };

    let tx_id = insert_double_entry_transaction(conn, tx_input)?;
    println!("Income recorded successfully! Transaction ID: {}", tx_id);
    println!("  - {}: -{}", source, format_money(amount_cents));
    println!("  - {}: +{}", account, format_money(amount_cents));

    Ok(())
}

pub fn handle_loan(
    conn: &mut Connection,
    principal: f64,
    total_amount: f64,
    installments: i64,
    bank_account: String,
    loan_account: String,
    interest_account: String,
    description: String,
    date: Option<String>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    if installments <= 0 {
        return Err("Number of installments must be greater than zero.".into());
    }

    let principal_cents = amount_to_cents(principal);
    let total_cents = amount_to_cents(total_amount);

    if total_cents < principal_cents {
        return Err("Total repayment amount cannot be less than principal.".into());
    }

    let currency = get_account_currency(conn, &bank_account)?;

    if find_account_id(conn, &loan_account).is_err() {
        println!("Creating missing loan account: '{}' (type: liability, currency: {})", loan_account, currency);
        handle_account_add(conn, loan_account.clone(), "liability".to_string(), currency.clone())?;
    }

    if find_account_id(conn, &interest_account).is_err() {
        println!("Creating missing interest account: '{}' (type: expense, currency: {})", interest_account, currency);
        handle_account_add(conn, interest_account.clone(), "expense".to_string(), currency.clone())?;
    }

    let total_interest = total_cents - principal_cents;
    let monthly_total = total_cents / installments;
    let remainder_total = total_cents % installments;
    let monthly_principal = principal_cents / installments;
    let remainder_principal = principal_cents % installments;

    let start_date = match date {
        Some(d) => NaiveDate::parse_from_str(&d, "%Y-%m-%d")
            .map_err(|_| "Invalid date format. Use YYYY-MM-DD.")?,
        None => chrono::Local::now().naive_local().date(),
    };

    let parent_group_id = Uuid::new_v4().to_string();

    println!("Recording loan of ${:.2} to be repaid in {} installments...", principal, installments);
    println!(
        "Principal: {} | Total Repayment: {} | Total Interest: {}", format_money(amount_to_cents(principal)), format_money(amount_to_cents(total_amount)), format_money(total_interest)
    );
    println!("------------------------------------------------------------");

    let tx_recv = TransactionInput {
        date: start_date.format("%Y-%m-%d").to_string(),
        description: format!("{} (Loan Received)", description),
        installment_current: None,
        installment_total: None,
        parent_group_id: Some(parent_group_id.clone()),
        entries: vec![
            EntryInput { account_name: bank_account.clone(), amount_native: principal_cents, exchange_rate: None },
            EntryInput { account_name: loan_account.clone(), amount_native: -principal_cents, exchange_rate: None },
        ],
    };
    let rx_id = insert_double_entry_transaction(conn, tx_recv)?;
    println!(
        "  [Received] {} -> Transaction ID {} | +{} to {}",
        start_date.format("%Y-%m-%d"),
        rx_id,
        format_money(principal_cents),
        bank_account
    );

    for i in 1..=installments {
        let current_monthly_total = if i == installments { monthly_total + remainder_total } else { monthly_total };
        let current_monthly_principal = if i == installments { monthly_principal + remainder_principal } else { monthly_principal };
        let current_monthly_interest = current_monthly_total - current_monthly_principal;

        let offset_months = Months::new(i as u32); 
        let project_date = start_date + offset_months;
        let project_date_str = project_date.format("%Y-%m-%d").to_string();

        let tx_desc = format!("{} (Repayment {}/{})", description, i, installments);

        let tx_input = TransactionInput {
            date: project_date_str.clone(),
            description: tx_desc,
            installment_current: Some(i),
            installment_total: Some(installments),
            parent_group_id: Some(parent_group_id.clone()),
            entries: vec![
                EntryInput { account_name: bank_account.clone(), amount_native: -current_monthly_total, exchange_rate: None },
                EntryInput { account_name: loan_account.clone(), amount_native: current_monthly_principal, exchange_rate: None },
                EntryInput { account_name: interest_account.clone(), amount_native: current_monthly_interest, exchange_rate: None },
            ],
        };

        let tx_id = insert_double_entry_transaction(conn, tx_input)?;
        println!(
            "  [Payment {}/{}] Projected for {} -> Transaction ID {} | -${:.2} (Principal: ${:.2}, Interest: ${:.2})", 
            i, installments, project_date_str, tx_id, 
            cents_to_amount(current_monthly_total), cents_to_amount(current_monthly_principal), cents_to_amount(current_monthly_interest)
        );
    }
    
    println!("------------------------------------------------------------");
    println!("Loan processing completed! UUID: {}", parent_group_id);

    Ok(())
}

pub fn handle_installments(
    conn: &mut Connection,
    cash_amount: f64,
    total_amount: f64,
    installments: i64,
    account: String,
    category: String,
    description: String,
    interest_account: String,
    date: Option<String>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    if installments <= 0 {
        return Err("Number of installments must be greater than zero.".into());
    }

    let cash_cents = amount_to_cents(cash_amount);
    let total_cents = amount_to_cents(total_amount);

    if total_cents < cash_cents {
        return Err("Total financed amount cannot be less than cash amount.".into());
    }

    let currency = get_account_currency(conn, &account)?;

    if find_account_id(conn, &interest_account).is_err() {
        println!("Creating missing interest account: '{}' (type: expense, currency: {})", interest_account, currency);
        handle_account_add(conn, interest_account.clone(), "expense".to_string(), currency)?;
    }

    let total_interest = total_cents - cash_cents;
    let monthly_total = total_cents / installments;
    let remainder_total = total_cents % installments;
    let monthly_cash = cash_cents / installments;
    let remainder_cash = cash_cents % installments;

    let parent_group_id = Uuid::new_v4().to_string();
    
    let start_date = match date {
        Some(d) => {
            NaiveDate::parse_from_str(&d, "%Y-%m-%d")
                .map_err(|_| "Invalid date format. Use YYYY-MM-DD.")?
        }
        None => chrono::Local::now().naive_local().date(),
    };

    println!("Recording financed purchase in {} installments...", installments);
    println!(
        "Cash Amount: {} | Total Amount: {} | Total Interest: {}", format_money(amount_to_cents(cash_amount)), format_money(amount_to_cents(total_amount)), format_money(total_interest)
    );
    println!("------------------------------------------------------------");

    for i in 1..=installments {
        let current_monthly_total = if i == installments { monthly_total + remainder_total } else { monthly_total };
        let current_monthly_cash = if i == installments { monthly_cash + remainder_cash } else { monthly_cash };
        let current_monthly_interest = current_monthly_total - current_monthly_cash;

        let offset_months = Months::new((i - 1) as u32);
        let project_date = start_date + offset_months;
        let project_date_str = project_date.format("%Y-%m-%d").to_string();

        let tx_desc = format!("{} (Installment {}/{})", description, i, installments);

        let tx_input = TransactionInput {
            date: project_date_str.clone(),
            description: tx_desc,
            installment_current: Some(i),
            installment_total: Some(installments),
            parent_group_id: Some(parent_group_id.clone()),
            entries: vec![
                EntryInput {
                    account_name: account.clone(),
                    amount_native: -current_monthly_total,
                    exchange_rate: None,
                },
                EntryInput {
                    account_name: category.clone(),
                    amount_native: current_monthly_cash,
                    exchange_rate: None,
                },
                EntryInput {
                    account_name: interest_account.clone(),
                    amount_native: current_monthly_interest,
                    exchange_rate: None,
                },
            ],
        };

        let tx_id = insert_double_entry_transaction(conn, tx_input)?;
        println!(
            "  [Installment {}/{}] Projected for {} -> Transaction ID {} | Installment: ${:.2} (Cash: ${:.2}, Interest: ${:.2})",
            i,
            installments,
            project_date_str,
            tx_id,
            cents_to_amount(current_monthly_total),
            cents_to_amount(current_monthly_cash),
            cents_to_amount(current_monthly_interest)
        );
    }

    println!("------------------------------------------------------------");
    println!("Installment projection completed and entered successfully!");
    println!("Installment Group UUID: {}", parent_group_id);

    Ok(())
}

pub fn handle_balance(
    conn: &Connection,
    from: Option<String>,
    to: Option<String>,
    month: Option<u32>,
    year: Option<i32>,
    live_rate: Option<String>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let (_final_from, final_to) = resolve_dates(from, to, month, year)?;

    let base_currency = get_base_currency(conn)?;

    let mut live_rates: HashMap<String, f64> = HashMap::new();
    if let Some(rates_str) = live_rate.as_ref() {
        for pair in rates_str.split(',') {
            let parts: Vec<&str> = pair.split(':').collect();
            if parts.len() == 2 {
                let curr = parts[0].trim().to_uppercase();
                if let Ok(rate) = parts[1].trim().parse::<f64>() {
                    live_rates.insert(curr, rate);
                }
            }
        }
    }

    let has_live_rates = live_rate.is_some();

    // Updated to query both amount_native and amount_base
    let mut stmt = conn.prepare(
        "SELECT a.name, a.type, a.currency, 
                COALESCE(SUM(e.amount_native), 0) as balance_native,
                COALESCE(SUM(e.amount_base), 0) as balance_base
         FROM accounts a
         LEFT JOIN entries e ON a.id = e.account_id
         LEFT JOIN transactions t ON e.transaction_id = t.id
         WHERE (a.type = 'asset' OR a.type = 'liability')
           AND (t.date <= ? OR t.id IS NULL)
         GROUP BY a.id, a.name, a.type, a.currency
         ORDER BY a.type ASC, a.name ASC;",
    )?;

    let rows = stmt.query_map(params![final_to], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
        ))
    })?;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS);
        
    let mut headers = vec![
        Cell::new("Account").fg(Color::Cyan),
        Cell::new("Type").fg(Color::Cyan),
        Cell::new("Currency").fg(Color::Cyan),
        Cell::new("Balance (Native)").fg(Color::Cyan),
    ];
    
    if has_live_rates {
        headers.push(Cell::new("Live Base Equiv.").fg(Color::Cyan));
    }
    table.set_header(headers);

    let mut count = 0;
    let mut total_assets_base_cents: i64 = 0;
    let mut total_liabilities_base_cents: i64 = 0;

    for row in rows {
        let (name, ty, currency, balance_native_cents, balance_historic_base_cents) = row?;
        count += 1;

        let balance_base_cents = if has_live_rates {
            // Apply live revaluation if requested
            let rate = if currency == base_currency {
                1.0
            } else {
                *live_rates.get(&currency).unwrap_or(&1.0) 
            };
            (balance_native_cents as f64 * rate).round() as i64
        } else {
            // Use historical base amount if no live rate
            balance_historic_base_cents
        };

        if ty == "asset" {
            total_assets_base_cents += balance_base_cents;
        } else if ty == "liability" {
            total_liabilities_base_cents += balance_base_cents;
        }

        let mut row_cells = vec![
            Cell::new(name),
            Cell::new(match ty.as_str() { "asset" => "Asset", "liability" => "Liability", _ => ty.as_str() }),
            Cell::new(currency),
        ];

        let formatted_native = format_money(balance_native_cents);
        let mut native_cell = Cell::new(formatted_native);
        if balance_native_cents > 0 { native_cell = native_cell.fg(Color::Green); }
        else if balance_native_cents < 0 { native_cell = native_cell.fg(Color::Red); }
        row_cells.push(native_cell);

        if has_live_rates {
            let formatted_base = format!("{} {}", format_money(balance_base_cents), base_currency);
            let mut base_cell = Cell::new(formatted_base);
            if balance_base_cents > 0 { base_cell = base_cell.fg(Color::Green); }
            else if balance_base_cents < 0 { base_cell = base_cell.fg(Color::Red); }
            row_cells.push(base_cell);
        }

        table.add_row(row_cells);
    }

    if count == 0 {
        println!("No accounts found to show in balance.");
        return Ok(());
    }

    println!("
=== ACCOUNT BALANCES (Up to {} | Excludes Future Installments) ===", final_to);
    println!("{}", table);

    let net_worth_base_cents = total_assets_base_cents + total_liabilities_base_cents;
    
    let summary_title = if has_live_rates { "LIVE REVALUATED NET WORTH" } else { "NET WORTH SUMMARY" };
    println!("
=== {} (in {}) ===", summary_title, base_currency);
    
    let mut summary_table = Table::new();
    summary_table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Category").fg(Color::Yellow),
            Cell::new("Total Equivalent").fg(Color::Yellow),
        ]);

    summary_table.add_row(vec![
        Cell::new("Total Assets"),
        Cell::new(format_money(total_assets_base_cents)).fg(Color::Green),
    ]);
    summary_table.add_row(vec![
        Cell::new("Total Liabilities"),
        Cell::new(format_money(total_liabilities_base_cents)).fg(Color::Red),
    ]);
    
    let mut net_cell = Cell::new(format_money(net_worth_base_cents));
    if net_worth_base_cents > 0 { net_cell = net_cell.fg(Color::Green); }
    else if net_worth_base_cents < 0 { net_cell = net_cell.fg(Color::Red); }
    
    summary_table.add_row(vec![
        Cell::new("Net Worth"),
        net_cell,
    ]);

    println!("{}", summary_table);

    Ok(())
}

pub fn handle_cashflow(
    conn: &Connection,
    from: Option<String>,
    to: Option<String>,
    month: Option<u32>,
    year: Option<i32>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let (final_from, final_to) = resolve_dates(from, to, month, year)?;

    let mut stmt = conn.prepare(
        "SELECT a.type, COALESCE(SUM(e.amount_base), 0) as total
         FROM accounts a
         JOIN entries e ON a.id = e.account_id
         JOIN transactions t ON e.transaction_id = t.id
         WHERE (a.type = 'income' OR a.type = 'expense')
           AND t.date >= ? AND t.date <= ?
         GROUP BY a.type;",
    )?;

    let mut total_income_cents: i64 = 0;
    let mut total_expense_cents: i64 = 0;

    let rows = stmt.query_map(params![final_from, final_to], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;

    for row in rows {
        let (ty, total) = row?;
        if ty == "income" {
            total_income_cents = -total;
        } else if ty == "expense" {
            total_expense_cents = total;
        }
    }

    let savings_cents = total_income_cents - total_expense_cents;
    let savings_rate = if total_income_cents > 0 {
        (savings_cents as f64 / total_income_cents as f64) * 100.0
    } else {
        0.0
    };

    println!("
=== INCOME STATEMENT (From {} to {}) ===", final_from, final_to);

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Financial Flow").fg(Color::Cyan),
            Cell::new("Base Amount").fg(Color::Cyan),
        ]);

    table.add_row(vec![
        Cell::new("(+) TOTAL INCOME").fg(Color::Green),
        Cell::new(format_money(total_income_cents)).fg(Color::Green),
    ]);
    table.add_row(vec![
        Cell::new("(-) TOTAL EXPENSES").fg(Color::Red),
        Cell::new(format_money(total_expense_cents)).fg(Color::Red),
    ]);

    let savings_color = if savings_cents >= 0 { Color::Green } else { Color::Red };
    let savings_text = if savings_cents >= 0 { "(=) NET SAVINGS [SAVED]" } else { "(=) NET DEFICIT [OVERSPENT]" };

    table.add_row(vec![
        Cell::new(savings_text).fg(savings_color),
        Cell::new(format_money(savings_cents)).fg(savings_color),
    ]);

    println!("{}", table);

    if total_income_cents > 0 {
        let percentage = savings_rate.clamp(0.0, 100.0);
        let blocks = (percentage / 4.0).round() as usize;
        let spaces = 25 - blocks;
        let bar = format!("{}{}", "█".repeat(blocks), "░".repeat(spaces));
        
        let color_code = if savings_rate >= 50.0 {
            "\x1b[32m" // Green
        } else if savings_rate >= 20.0 {
            "\x1b[33m" // Yellow
        } else {
            "\x1b[31m" // Red
        };
        println!("{}Savings Rate: {:.2}%  {}\x1b[0m", color_code, savings_rate, bar);
    } else if total_expense_cents > 0 {
        println!("\x1b[31mSavings Rate: 0.00% (No income recorded in this period)\x1b[0m");
    } else {
        println!("No income or expense transactions recorded in this period.");
    }

    Ok(())
}

pub fn handle_expense_report(
    conn: &Connection,
    from: Option<String>,
    to: Option<String>,
    month: Option<u32>,
    year: Option<i32>,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let (final_from, final_to) = resolve_dates(from, to, month, year)?;

    let mut stmt = conn.prepare(
        "SELECT a.id, a.name, COALESCE(SUM(e.amount_base), 0) as total, b.amount_limit
         FROM accounts a
         JOIN entries e ON a.id = e.account_id
         JOIN transactions t ON e.transaction_id = t.id
         LEFT JOIN budgets b ON a.id = b.account_id
         WHERE a.type = 'expense'
           AND t.date >= ? AND t.date <= ?
         GROUP BY a.id, a.name
         HAVING total > 0
         ORDER BY total DESC;",
    )?;

    let rows = stmt.query_map(params![final_from, final_to], |row| {
        Ok((
            row.get::<_, String>(1)?, 
            row.get::<_, i64>(2)?,
            row.get::<_, Option<i64>>(3)?,
        ))
    })?;

    let mut categories = Vec::new();
    let mut grand_total_cents: i64 = 0;

    for row in rows {
        let (name, total, limit) = row?;
        grand_total_cents += total;
        categories.push((name, total, limit));
    }

    println!("
=== EXPENSE DISTRIBUTION (From {} to {}) ===", final_from, final_to);
    println!("Total Expenses (Base Currency): {}", format_money(grand_total_cents));
    println!("------------------------------------------------------------");

    if grand_total_cents == 0 {
        println!("No expenses recorded in this period.");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Base Amount").fg(Color::Cyan),
            Cell::new("Percentage").fg(Color::Cyan),
            Cell::new("Budget Status").fg(Color::Cyan),
            Cell::new("Distribution Graph").fg(Color::Cyan),
        ]);

    for (name, total, limit_opt) in categories {
        let percentage = (total as f64 / grand_total_cents as f64) * 100.0;
        let blocks = (percentage / 4.0).round() as usize;
        let spaces = 25 - blocks;
        let bar = format!("{}{}", "█".repeat(blocks), "░".repeat(spaces));

        let color = if percentage >= 40.0 {
            Color::Red
        } else if percentage >= 15.0 {
            Color::Yellow
        } else {
            Color::Green
        };

        let budget_status = match limit_opt {
            Some(limit_cents) => {
                let consumption = (total as f64 / limit_cents as f64) * 100.0;
                if total > limit_cents {
                    Cell::new(format!("OVER BUDGET! ({:.1}%)", consumption)).fg(Color::Red)
                } else {
                    Cell::new(format!("{:.1}% used", consumption)).fg(Color::Green)
                }
            }
            None => Cell::new("N/A").fg(Color::DarkGrey),
        };

        table.add_row(vec![
            Cell::new(name),
            Cell::new(format_money(total)),
            Cell::new(format!("{:.2}%", percentage)).fg(color),
            budget_status,
            Cell::new(bar).fg(color),
        ]);
    }

    println!("{}", table);

    Ok(())
}

pub fn handle_history_report(
    conn: &Connection,
    months: u32,
) -> std::result::Result<(), Box<dyn Error>> {
    check_database_initialized(conn)?;

    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m', t.date) as month,
                SUM(CASE WHEN a.type = 'income' THEN -e.amount_base ELSE 0 END) as income_cents,
                SUM(CASE WHEN a.type = 'expense' THEN e.amount_base ELSE 0 END) as expense_cents
         FROM entries e
         JOIN accounts a ON e.account_id = a.id
         JOIN transactions t ON e.transaction_id = t.id
         GROUP BY month
         ORDER BY month DESC
         LIMIT ?;",
    )?;

    let rows = stmt.query_map(params![months], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;

    let mut data = Vec::new();
    for row in rows {
        data.push(row?);
    }

    data.reverse();

    println!("
=== MONTHLY HISTORICAL EVOLUTION (Last {} months) ===", months);

    if data.is_empty() {
        println!("Not enough data to generate historical report.");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("Month").fg(Color::Cyan),
            Cell::new("Income (Base)").fg(Color::Green),
            Cell::new("Expenses (Base)").fg(Color::Red),
            Cell::new("Net Savings").fg(Color::Cyan),
            Cell::new("% Savings").fg(Color::Cyan),
        ]);

    for (month, income_cents, expense_cents) in data {
        let savings_cents = income_cents - expense_cents;
        let savings_rate = if income_cents > 0 {
            (savings_cents as f64 / income_cents as f64) * 100.0
        } else {
            0.0
        };

        let savings_color = if savings_cents >= 0 { Color::Green } else { Color::Red };
        
        table.add_row(vec![
            Cell::new(month),
            Cell::new(format_money(income_cents)).fg(Color::Green),
            Cell::new(format_money(expense_cents)).fg(Color::Red),
            Cell::new(format_money(savings_cents)).fg(savings_color),
            Cell::new(format!("{:.2}%", savings_rate)).fg(savings_color),
        ]);
    }

    println!("{}", table);

    Ok(())
}
