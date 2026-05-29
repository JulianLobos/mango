// src/main.rs
use clap::Parser;

mod cli;
mod db;
mod handlers;
mod models;
mod utils;
mod tests;

use cli::{Cli, Commands};
use db::establish_connection;

fn main() {
    let cli = Cli::parse();

    let subcommand = match cli.command {
        Some(cmd) => cmd,
        None => {
            use clap::CommandFactory;
            let mut cmd = Cli::command();
            cmd.print_help().unwrap();
            println!();
            return;
        }
    };

    let mut conn = match establish_connection(&cli.database) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error connecting to database: {}", e);
            std::process::exit(1);
        }
    };

    let result = match subcommand {
        Commands::Setup { base_currency } => handlers::handle_setup(&conn, base_currency),
        Commands::Expense { amount, account, category, description, date } => 
            handlers::handle_expense(&mut conn, amount, account, category, description, date),
        Commands::Income { amount, account, source, description, date } => 
            handlers::handle_income(&mut conn, amount, account, source, description, date),
        Commands::Installments { cash_amount, total_amount, installments, account, category, description, interest_account, date } => 
            handlers::handle_installments(&mut conn, cash_amount, total_amount, installments, account, category, description, interest_account, date),
        Commands::Loan { principal, total_amount, installments, bank_account, loan_account, interest_account, description, date } => 
            handlers::handle_loan(&mut conn, principal, total_amount, installments, bank_account, loan_account, interest_account, description, date),
        Commands::Balance { from, to, month, year, live_rate } => 
            handlers::handle_balance(&conn, from, to, month, year, live_rate),
        Commands::Cashflow { from, to, month, year } => 
            handlers::handle_cashflow(&conn, from, to, month, year),
        Commands::ExpenseReport { from, to, month, year } => 
            handlers::handle_expense_report(&conn, from, to, month, year),
        Commands::HistoryReport { months } => 
            handlers::handle_history_report(&conn, months),
        Commands::Account { cmd } => 
            handlers::handle_account(&mut conn, cmd),
        Commands::List { limit, month, year } => 
            handlers::handle_list(&conn, limit, month, year),
        Commands::Find { query } => 
            handlers::handle_find(&conn, query),
        Commands::Transfer { amount, from, to, description, date, exchange_rate, cost_amount } => 
            handlers::handle_transfer(&mut conn, amount, from, to, description, date, exchange_rate, cost_amount),
        Commands::Delete { id } => 
            handlers::handle_delete(&conn, id),
        Commands::Edit { id, description, amount, date } => 
            handlers::handle_edit(&mut conn, id, description, amount, date),
        Commands::Export { file } => 
            handlers::handle_export(&conn, &file),
        Commands::Budget { cmd } => 
            handlers::handle_budget(&mut conn, cmd),
        Commands::PayLiability { account, from, month, year, date } =>
            handlers::handle_pay_liability(&mut conn, account, from, month, year, date),
    };

    if let Err(e) = result {
        eprintln!("\nExecution error: {}", e);
        std::process::exit(1);
    }
}
