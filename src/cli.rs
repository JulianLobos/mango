// src/cli.rs
use clap::{Parser, Subcommand};

pub const ASCII_ART: &str = "
\x1b[38;5;214m _ __ ___   __ _ _ __   __ _  ___  
| '_ ` _ \\ / _` | '_ \\ / _` |/ _ \\ 
| | | | | | (_| | | | | (_| | (_) |
|_| |_| |_|\\__,_|_| |_|\\__, |\\___/ 
                        __/ |      
                       |___/       \x1b[0m
";

#[derive(Parser)]
#[command(name = "mango")]
#[command(version = "0.1.0")]
#[command(author = "Julian")]
#[command(about = "Double-entry CLI personal finance application", long_about = None)]
#[command(before_help = ASCII_ART)]
pub struct Cli {
    #[arg(short, long, default_value = "mango.db", help = "Path to the SQLite database file")]
    pub database: String,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Initialize the database and create default accounts")]
    Setup {
        #[arg(long, default_value = "USD", help = "The primary currency for your accounting system (e.g., USD, ARS, EUR)")]
        base_currency: String,
    },

    #[command(about = "Record a simple expense (Creates 1 transaction and 2 entries)")]
    Expense {
        #[arg(long, help = "Amount in decimal format (e.g., 150.50)")]
        amount: f64,

        #[arg(long, help = "Source account name (e.g., Cash, Bank)")]
        account: String,

        #[arg(long, help = "Destination category name (e.g., Groceries, Rent)")]
        category: String,

        #[arg(long, help = "Description of the expense")]
        description: String,

        #[arg(long, help = "Optional date (YYYY-MM-DD). Defaults to today.")]
        date: Option<String>,
    },

    #[command(about = "Record incoming money (Creates 1 transaction and 2 entries)")]
    Income {
        #[arg(long, help = "Amount in decimal format (e.g., 5000.00)")]
        amount: f64,

        #[arg(long, help = "Destination account name (e.g., Bank, Cash)")]
        account: String,

        #[arg(long, help = "Source income account name (e.g., Salary)")]
        source: String,

        #[arg(long, help = "Description of the income")]
        description: String,

        #[arg(long, help = "Optional date (YYYY-MM-DD). Defaults to today.")]
        date: Option<String>,
    },

    #[command(about = "Record a financed purchase in installments projecting future interest")]
    Installments {
        #[arg(long, help = "Cash/spot value of the product in decimal format")]
        cash_amount: f64,

        #[arg(long, help = "Total financed amount to pay in decimal format")]
        total_amount: f64,

        #[arg(long, help = "Number of monthly installments")]
        installments: i64,

        #[arg(long, help = "Liability account name (e.g., Credit Card)")]
        account: String,

        #[arg(long, help = "Destination expense category name (e.g., Groceries, Tech)")]
        category: String,

        #[arg(long, help = "Description of the purchase")]
        description: String,

        #[arg(long, default_value = "Interest", help = "Account name for financing interest (will be created if missing)")]
        interest_account: String,

        #[arg(long, help = "Optional starting date (YYYY-MM-DD). Defaults to today.")]
        date: Option<String>,
    },

    #[command(about = "Record receiving a loan and project its future installment repayments")]
    Loan {
        #[arg(long, help = "Principal (capital) amount received")]
        principal: f64,

        #[arg(long, help = "Total amount to repay (principal + interest)")]
        total_amount: f64,

        #[arg(long, help = "Number of monthly repayment installments")]
        installments: i64,

        #[arg(long, help = "Asset account receiving the loan and paying installments (e.g., Bank)")]
        bank_account: String,

        #[arg(long, help = "Liability account for the loan (e.g., Personal Loan)")]
        loan_account: String,

        #[arg(long, default_value = "Interest", help = "Expense account for financing interest (will be created if missing)")]
        interest_account: String,

        #[arg(long, help = "Description of the loan")]
        description: String,

        #[arg(long, help = "Optional starting date (YYYY-MM-DD). Defaults to today.")]
        date: Option<String>,
    },

    #[command(about = "Show account balances for asset and liability types with optional time filters")]
    Balance {
        #[arg(long, help = "Start date (YYYY-MM-DD, e.g. 2026-01-01)")]
        from: Option<String>,

        #[arg(long, help = "End date (YYYY-MM-DD, e.g. 2026-05-31)")]
        to: Option<String>,

        #[arg(long, help = "Filter by specific month (1-12)")]
        month: Option<u32>,

        #[arg(long, help = "Filter by specific year (e.g., 2026)")]
        year: Option<i32>,

        #[arg(long, help = "Live exchange rates for revaluation (e.g. 'USD:1500,EUR:1600')")]
        live_rate: Option<String>,
    },

    #[command(about = "Show a cash flow report (Income vs Expenses) for a period")]
    Cashflow {
        #[arg(long, help = "Start date (YYYY-MM-DD, e.g. 2026-01-01)")]
        from: Option<String>,

        #[arg(long, help = "End date (YYYY-MM-DD, e.g. 2026-05-31)")]
        to: Option<String>,

        #[arg(long, help = "Filter by specific month (1-12)")]
        month: Option<u32>,

        #[arg(long, help = "Filter by specific year (e.g., 2026)")]
        year: Option<i32>,
    },

    #[command(about = "Show percentage distribution of expenses by category")]
    ExpenseReport {
        #[arg(long, help = "Start date (YYYY-MM-DD, e.g. 2026-01-01)")]
        from: Option<String>,

        #[arg(long, help = "End date (YYYY-MM-DD, e.g. 2026-05-31)")]
        to: Option<String>,

        #[arg(long, help = "Filter by specific month (1-12)")]
        month: Option<u32>,

        #[arg(long, help = "Filter by specific year (e.g., 2026)")]
        year: Option<i32>,
    },

    #[command(about = "Show monthly historical evolution of income, expenses, and savings")]
    HistoryReport {
        #[arg(long, default_value = "12", help = "Number of historical months to show")]
        months: u32,
    },

    #[command(about = "Manage your accounts (Add, List, Update)")]
    Account {
        #[command(subcommand)]
        cmd: AccountCommands,
    },

    #[command(about = "List recent transactions, with optional time filters")]
    List {
        #[arg(long, default_value = "20", help = "Number of transactions to show (ignored if month or year is specified)")]
        limit: u32,

        #[arg(long, help = "Filter by specific month (1-12)")]
        month: Option<u32>,

        #[arg(long, help = "Filter by specific year (e.g., 2026)")]
        year: Option<i32>,
    },

    #[command(about = "Find transactions by description")]
    Find {
        #[arg(long, help = "Text to search for in descriptions")]
        query: String,
    },

    #[command(about = "Transfer money between accounts")]
    Transfer {
        #[arg(long, help = "Amount to transfer in decimal format")]
        amount: f64,

        #[arg(long, help = "Source account name")]
        from: String,

        #[arg(long, help = "Destination account name")]
        to: String,

        #[arg(long, help = "Description of the transfer")]
        description: String,

        #[arg(long, help = "Optional date (YYYY-MM-DD). Defaults to today.")]
        date: Option<String>,

        #[arg(long, help = "Exchange rate (if transferring between different currencies)")]
        exchange_rate: Option<f64>,

        #[arg(long, help = "Total cost in destination currency (alternative to exchange-rate)")]
        cost_amount: Option<f64>,
    },

    #[command(about = "Delete a transaction by ID")]
    Delete {
        #[arg(long, help = "ID of the transaction to delete")]
        id: i64,
    },

    #[command(about = "Edit an existing transaction (Only simple 2-entry transactions for amount)")]
    Edit {
        #[arg(long, help = "ID of the transaction to edit")]
        id: i64,

        #[arg(long, help = "New description (optional)")]
        description: Option<String>,

        #[arg(long, help = "New amount (optional, only for simple 2-entry transactions)")]
        amount: Option<f64>,

        #[arg(long, help = "New date (optional, YYYY-MM-DD)")]
        date: Option<String>,
    },

    #[command(about = "Export all transactions and entries to a CSV file")]
    Export {
        #[arg(long, default_value = "mango_export.csv", help = "Filename for the CSV export")]
        file: String,
    },

    #[command(about = "Manage monthly budget limits for expense categories")]
    Budget {
        #[command(subcommand)]
        cmd: BudgetCommands,
    },

    #[command(about = "Settle a month's debt for a liability account (e.g., pay credit card)")]
    PayLiability {
        #[arg(long, help = "The liability account to pay off (e.g., 'Tarjeta de Crédito')")]
        account: String,

        #[arg(long, help = "The asset account to pay from (e.g., 'Banco')")]
        from: String,

        #[arg(long, help = "The month of the debt to settle (1-12)")]
        month: u32,

        #[arg(long, help = "The year of the debt to settle (e.g., 2026)")]
        year: i32,

        #[arg(long, help = "Optional payment date (YYYY-MM-DD). Defaults to today.")]
        date: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum BudgetCommands {
    #[command(about = "Set or update a monthly limit for a category")]
    Set {
        #[arg(long, help = "Expense category name")]
        category: String,

        #[arg(long, help = "Monthly limit amount in decimal format")]
        limit: f64,
    },

    #[command(about = "List all active budget limits")]
    List,

    #[command(about = "Remove a budget limit from a category")]
    Delete {
        #[arg(long, help = "Expense category name")]
        category: String,
    },
}

#[derive(Subcommand)]
pub enum AccountCommands {
    #[command(about = "Add a new account")]
    Add {
        #[arg(long, help = "Name of the account")]
        name: String,

        #[arg(long, help = "Type of account (asset, liability, income, expense)")]
        r#type: String,

        #[arg(long, default_value = "USD", help = "Currency of the account (e.g., USD, EUR, ARS)")]
        currency: String,
    },

    #[command(about = "List all accounts")]
    List,

    #[command(about = "Update an existing account name or currency")]
    Update {
        #[arg(long, help = "Current name of the account")]
        old_name: String,

        #[arg(long, help = "New name for the account (optional)")]
        new_name: Option<String>,

        #[arg(long, help = "New currency for the account (optional)")]
        currency: Option<String>,
    },

    #[command(about = "Delete an account (Only if it has no transactions)")]
    Delete {
        #[arg(long, help = "Name of the account to delete")]
        name: String,
    },

    #[command(about = "Adjust the balance of an asset account to a new value")]
    Adjust {
        #[arg(long, help = "Name of the asset account to adjust")]
        name: String,

        #[arg(long, help = "The new total balance for the account")]
        new_balance: f64,

        #[arg(long, help = "A description for the adjustment transaction")]
        description: Option<String>,

        #[arg(long, help = "Exchange rate for valuing adjustments on non-base currency accounts")]
        exchange_rate: Option<f64>,
    },
}
