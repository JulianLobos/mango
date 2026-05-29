// src/db.rs
use rusqlite::{params, Connection, Result};
use std::error::Error;
use crate::models::TransactionInput;

pub fn establish_connection(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    // Enforce SQLite Foreign Key constraints for integrity!
    conn.execute("PRAGMA foreign_keys = ON;", [])?;
    Ok(conn)
}

pub fn create_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS accounts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT UNIQUE NOT NULL,
            type TEXT NOT NULL CHECK(type IN ('asset', 'liability', 'income', 'expense', 'equity')),
            currency TEXT NOT NULL DEFAULT 'USD'
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS transactions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            date TEXT NOT NULL,
            description TEXT NOT NULL,
            installment_current INTEGER,
            installment_total INTEGER,
            parent_group_id TEXT
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS entries (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            transaction_id INTEGER NOT NULL,
            account_id INTEGER NOT NULL,
            amount_native INTEGER NOT NULL,
            amount_base INTEGER NOT NULL,
            exchange_rate REAL NOT NULL DEFAULT 1.0,
            FOREIGN KEY(transaction_id) REFERENCES transactions(id) ON DELETE CASCADE,
            FOREIGN KEY(account_id) REFERENCES accounts(id)
        );",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS budgets (
            account_id INTEGER PRIMARY KEY,
            amount_limit INTEGER NOT NULL,
            FOREIGN KEY(account_id) REFERENCES accounts(id) ON DELETE CASCADE
        );",
        [],
    )?;

    Ok(())
}

pub fn find_account_id(conn: &Connection, name: &str) -> std::result::Result<i64, Box<dyn Error>> {
    let result: Result<i64> = conn.query_row(
        "SELECT id FROM accounts WHERE name = ? COLLATE NOCASE;",
        params![name],
        |row| row.get(0),
    );

    match result {
        Ok(id) => Ok(id),
        Err(_) => Err(format!(
            "Account '{}' not found in database. Run 'mango setup' or check the name.",
            name
        )
        .into()),
    }
}

pub fn get_account_currency(conn: &Connection, name: &str) -> std::result::Result<String, Box<dyn Error>> {
    let result: Result<String> = conn.query_row(
        "SELECT currency FROM accounts WHERE name = ? COLLATE NOCASE;",
        params![name],
        |row| row.get(0),
    );

    match result {
        Ok(curr) => Ok(curr),
        Err(_) => Ok("USD".to_string()), // Fallback if not found
    }
}

pub fn get_base_currency(conn: &Connection) -> std::result::Result<String, Box<dyn Error>> {
    let result: Result<String> = conn.query_row(
        "SELECT value FROM settings WHERE key = 'base_currency';",
        [],
        |row| row.get(0),
    );

    match result {
        Ok(curr) => Ok(curr),
        Err(_) => Ok("USD".to_string()), // Default base currency if not explicitly set
    }
}

pub fn insert_double_entry_transaction(
    conn: &mut Connection,
    tx_input: TransactionInput,
) -> std::result::Result<i64, Box<dyn Error>> {
    // We need the base currency to determine default exchange rates for implicit operations
    let base_currency = get_base_currency(conn)?;

    // 1. Calculate amount_base and Critical Pre-Check: Do all base entries sum to exactly zero?
    let mut sum_base: i64 = 0;
    
    // We use a transaction not just for inserts, but also to safely query account currencies mid-flight
    let tx = conn.transaction()?;

    // Pre-calculate base amounts and check integrity BEFORE doing any inserts
    let mut prepared_entries = Vec::new();

    for entry in &tx_input.entries {
        let account_id = find_account_id(&tx, &entry.account_name)?;
        let acc_currency = get_account_currency(&tx, &entry.account_name)?;
        
        let mut exchange_rate = 1.0;
        let mut amount_base = entry.amount_native;

        if acc_currency != base_currency {
            if let Some(explicit_rate) = entry.exchange_rate {
                exchange_rate = explicit_rate;
                amount_base = (entry.amount_native as f64 * exchange_rate).round() as i64;
            } else {
                return Err(format!(
                    "Cross-currency transaction detected! Account '{}' is in {} but system base is {}. You must provide an exchange rate.",
                    entry.account_name, acc_currency, base_currency
                ).into());
            }
        }

        sum_base += amount_base;
        prepared_entries.push((account_id, entry.amount_native, amount_base, exchange_rate));
    }

    if sum_base != 0 {
        return Err(format!(
            "Integrity Rejected: Entry sum (in Base Currency) must be exactly 0. Calculated sum: {} base cents.",
            sum_base
        )
        .into());
    }

    // 2. Insert Transaction Header
    tx.execute(
        "INSERT INTO transactions (date, description, installment_current, installment_total, parent_group_id)
         VALUES (?, ?, ?, ?, ?);",
        params![
            tx_input.date,
            tx_input.description,
            tx_input.installment_current,
            tx_input.installment_total,
            tx_input.parent_group_id,
        ],
    )?;

    let transaction_id = tx.last_insert_rowid();

    // 3. Insert Entries
    for (account_id, amount_native, amount_base, exchange_rate) in prepared_entries {
        tx.execute(
            "INSERT INTO entries (transaction_id, account_id, amount_native, amount_base, exchange_rate) VALUES (?, ?, ?, ?, ?);",
            params![transaction_id, account_id, amount_native, amount_base, exchange_rate],
        )?;
    }

    // 4. Hard SQLite-level Check: Query database to double-check that base sum is 0
    let db_sum: i64 = tx.query_row(
        "SELECT SUM(amount_base) FROM entries WHERE transaction_id = ?;",
        params![transaction_id],
        |row| row.get(0),
    )?;

    if db_sum != 0 {
        tx.rollback()?;
        return Err(format!(
            "Critical Integrity Error in SQLite: Transaction {} entries base sum to {} cents in DB. Transaction aborted.",
            transaction_id, db_sum
        )
        .into());
    }

    tx.commit()?;

    Ok(transaction_id)
}

pub fn check_database_initialized(conn: &Connection) -> std::result::Result<(), Box<dyn Error>> {
    let table_exists: Result<i32> = conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type='table' AND name='accounts';",
        [],
        |row| row.get(0),
    );

    if table_exists.is_err() {
        return Err(
            "Database is not initialized. Please run first: 'mango setup'".into(),
        );
    }
    Ok(())
}
