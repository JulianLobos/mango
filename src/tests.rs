// src/tests.rs
#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use crate::db::*;
    use crate::models::{EntryInput, TransactionInput};
    use crate::utils::*;
    use std::error::Error;
    use crate::handlers::handle_account_add;

    #[test]
    fn test_cents_conversion() {
        assert_eq!(amount_to_cents(15.50), 1550);
        assert_eq!(amount_to_cents(100.00), 10000);
        assert_eq!(amount_to_cents(0.00), 0);
        assert_eq!(cents_to_amount(1550), 15.50);
        assert_eq!(cents_to_amount(10000), 100.00);
        assert_eq!(cents_to_amount(0), 0.00);
    }

    #[test]
    fn test_cents_precision_rounding() {
        assert_eq!(amount_to_cents(10.29), 1029);
        assert_eq!(amount_to_cents(10.293), 1029);
        assert_eq!(amount_to_cents(10.295), 1030);
    }

    #[test]
    fn test_db_setup_and_double_entry_integrity() -> std::result::Result<(), Box<dyn Error>> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("PRAGMA foreign_keys = ON;", [])?;
        create_schema(&conn)?;
        
        conn.execute("INSERT INTO settings (key, value) VALUES ('base_currency', 'USD');", [])?;
        
        handle_account_add(&conn, "Cash".to_string(), "asset".to_string(), "USD".to_string())?;
        handle_account_add(&conn, "Groceries".to_string(), "expense".to_string(), "USD".to_string())?;

        let tx_valid = TransactionInput {
            date: "2026-05-25".to_string(),
            description: "Valid Transaction".to_string(),
            installment_current: None,
            installment_total: None,
            parent_group_id: None,
            entries: vec![
                EntryInput { account_name: "Cash".to_string(), amount_native: -1000, exchange_rate: None },
                EntryInput { account_name: "Groceries".to_string(), amount_native: 1000, exchange_rate: None },
            ],
        };

        let tx_id = insert_double_entry_transaction(&mut conn, tx_valid);
        assert!(tx_id.is_ok());

        let tx_invalid = TransactionInput {
            date: "2026-05-25".to_string(),
            description: "Invalid Transaction".to_string(),
            installment_current: None,
            installment_total: None,
            parent_group_id: None,
            entries: vec![
                EntryInput { account_name: "Cash".to_string(), amount_native: -1000, exchange_rate: None },
                EntryInput { account_name: "Groceries".to_string(), amount_native: 999, exchange_rate: None }, // Off by 1
            ],
        };

        let tx_id_err = insert_double_entry_transaction(&mut conn, tx_invalid);
        assert!(tx_id_err.is_err());
        assert!(tx_id_err.unwrap_err().to_string().contains("Integrity Rejected"));

        Ok(())
    }

    #[test]
    fn test_installments_remainder_split() {
        let total_cents = 120000;
        let cash_cents = 100000;
        let installments = 3;

        let monthly_total = total_cents / installments;
        let remainder_total = total_cents % installments;

        let monthly_cash = cash_cents / installments;
        let remainder_cash = cash_cents % installments;

        assert_eq!(monthly_total, 40000);
        assert_eq!(remainder_total, 0);

        assert_eq!(monthly_cash, 33333);
        assert_eq!(remainder_cash, 1);

        let mut reconstructed_total = 0;
        let mut reconstructed_cash = 0;
        let mut reconstructed_interest = 0;

        for i in 1..=installments {
            let cur_total = if i == installments { monthly_total + remainder_total } else { monthly_total };
            let cur_cash = if i == installments { monthly_cash + remainder_cash } else { monthly_cash };
            let cur_interest = cur_total - cur_cash;

            reconstructed_total += cur_total;
            reconstructed_cash += cur_cash;
            reconstructed_interest += cur_interest;
        }

        assert_eq!(reconstructed_total, total_cents);
        assert_eq!(reconstructed_cash, cash_cents);
        assert_eq!(reconstructed_interest, total_cents - cash_cents);
    }

    #[test]
    fn test_resolve_dates() {
        let res = resolve_dates(
            Some("2026-01-15".to_string()),
            Some("2026-03-22".to_string()),
            None,
            None,
        );
        assert!(res.is_ok());
        let (d, h) = res.unwrap();
        assert_eq!(d, "2026-01-15");
        assert_eq!(h, "2026-03-22");

        let res_leap = resolve_dates(None, None, Some(2), Some(2024));
        assert!(res_leap.is_ok());
        let (d_leap, h_leap) = res_leap.unwrap();
        assert_eq!(d_leap, "2024-02-01");
        assert_eq!(h_leap, "2024-02-29");

        let res_non_leap = resolve_dates(None, None, Some(2), Some(2025));
        assert!(res_non_leap.is_ok());
        let (d_nl, h_nl) = res_non_leap.unwrap();
        assert_eq!(d_nl, "2025-02-01");
        assert_eq!(h_nl, "2025-02-28");

        let res_dec = resolve_dates(None, None, Some(12), Some(2026));
        assert!(res_dec.is_ok());
        let (d_dec, h_dec) = res_dec.unwrap();
        assert_eq!(d_dec, "2026-12-01");
        assert_eq!(h_dec, "2026-12-31");
    }

    #[test]
    fn test_grouped_transactions_deletion() -> std::result::Result<(), Box<dyn Error>> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("PRAGMA foreign_keys = ON;", [])?;
        create_schema(&conn)?;
        
        conn.execute("INSERT INTO settings (key, value) VALUES ('base_currency', 'USD');", [])?;
        
        handle_account_add(&conn, "Credit Card".to_string(), "liability".to_string(), "USD".to_string())?;
        handle_account_add(&conn, "Tech".to_string(), "expense".to_string(), "USD".to_string())?;

        let group_id = "test-group-12345".to_string();

        // Prepare two grouped transactions
        let tx1 = TransactionInput {
            date: "2026-05-25".to_string(),
            description: "Financed purchase (Installment 1/2)".to_string(),
            installment_current: Some(1),
            installment_total: Some(2),
            parent_group_id: Some(group_id.clone()),
            entries: vec![
                EntryInput { account_name: "Credit Card".to_string(), amount_native: -5000, exchange_rate: None },
                EntryInput { account_name: "Tech".to_string(), amount_native: 5000, exchange_rate: None },
            ],
        };

        let tx2 = TransactionInput {
            date: "2026-06-25".to_string(),
            description: "Financed purchase (Installment 2/2)".to_string(),
            installment_current: Some(2),
            installment_total: Some(2),
            parent_group_id: Some(group_id.clone()),
            entries: vec![
                EntryInput { account_name: "Credit Card".to_string(), amount_native: -5000, exchange_rate: None },
                EntryInput { account_name: "Tech".to_string(), amount_native: 5000, exchange_rate: None },
            ],
        };

        let tx_id1 = insert_double_entry_transaction(&mut conn, tx1)?;
        let _tx_id2 = insert_double_entry_transaction(&mut conn, tx2)?;

        // Verify that two transactions exist in the group
        let count_before: i64 = conn.query_row("SELECT COUNT(*) FROM transactions WHERE parent_group_id = 'test-group-12345';", [], |row| row.get(0))?;
        assert_eq!(count_before, 2);

        // Delete the first transaction
        crate::handlers::handle_delete(&conn, tx_id1)?;

        // Verify that both transactions in the group were deleted
        let count_after: i64 = conn.query_row("SELECT COUNT(*) FROM transactions WHERE parent_group_id = 'test-group-12345';", [], |row| row.get(0))?;
        assert_eq!(count_after, 0);

        Ok(())
    }

    #[test]
    fn test_loan_generation_and_payment() -> std::result::Result<(), Box<dyn Error>> {
        let mut conn = Connection::open_in_memory()?;
        conn.execute("PRAGMA foreign_keys = ON;", [])?;
        create_schema(&conn)?;
        
        conn.execute("INSERT INTO settings (key, value) VALUES ('base_currency', 'USD');", [])?;
        
        handle_account_add(&conn, "Bank".to_string(), "asset".to_string(), "USD".to_string())?;
        handle_account_add(&conn, "Personal Loan".to_string(), "liability".to_string(), "USD".to_string())?;
        handle_account_add(&conn, "Interest".to_string(), "expense".to_string(), "USD".to_string())?;

        // Record a loan of $10,000 to be repaid in 10 installments of $1,100 total, starting repayment on 2026-05-25
        crate::handlers::handle_loan(
            &mut conn,
            10000.0,
            11000.0,
            10,
            "Bank".to_string(),
            "Personal Loan".to_string(),
            "Interest".to_string(),
            "Test Loan".to_string(),
            Some("2026-05-25".to_string()),
            Some("2026-05-25".to_string()),
        )?;

        // Verify loan principal reception: Bank should have $10,000
        let bank_balance_after_recv: i64 = conn.query_row(
            "SELECT COALESCE(SUM(amount_native), 0) FROM entries WHERE account_id = (SELECT id FROM accounts WHERE name = 'Bank');",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(bank_balance_after_recv, 1000000); // 10,000 * 100 cents

        // Run pay-liability for Month 5 (May) Year 2026 to pay installment 1 (due 2026-05-25) on the Installments account
        // Installment total: $11,000 / 10 = $1,100 ($110,000 cents)
        crate::handlers::handle_pay_liability(
            &mut conn,
            "Personal Loan (Installments)".to_string(),
            "Bank".to_string(),
            5,
            2026,
            Some("2026-05-25".to_string()),
        )?;

        // Bank balance should be reduced by $1,100 to $8,900
        let bank_balance_after_payment: i64 = conn.query_row(
            "SELECT COALESCE(SUM(amount_native), 0) FROM entries WHERE account_id = (SELECT id FROM accounts WHERE name = 'Bank');",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(bank_balance_after_payment, 890000); // 8,900 * 100 cents

        // The payment transaction was created successfully. Let's find its transaction ID!
        let payment_tx_id: i64 = conn.query_row(
            "SELECT id FROM transactions WHERE description = 'Payment for Personal Loan (Installments) statement 5/2026';",
            [],
            |row| row.get(0),
        )?;

        // Try deleting the payment first. It should delete ONLY the payment transaction, leaving the loan group intact!
        crate::handlers::handle_delete(&conn, payment_tx_id)?;

        // Verify the payment transaction is deleted
        let payment_exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM transactions WHERE id = ?);",
            [payment_tx_id],
            |row| row.get(0),
        )?;
        assert!(!payment_exists);

        // Verify that the loan receipt still exists (Bank balance is back to $10,000 since payment is deleted)
        let bank_balance_after_payment_delete: i64 = conn.query_row(
            "SELECT COALESCE(SUM(amount_native), 0) FROM entries WHERE account_id = (SELECT id FROM accounts WHERE name = 'Bank');",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(bank_balance_after_payment_delete, 1000000); // Back to $10,000

        // Now, let's record the payment again so we can test full cascaded deletion
        crate::handlers::handle_pay_liability(
            &mut conn,
            "Personal Loan (Installments)".to_string(),
            "Bank".to_string(),
            5,
            2026,
            Some("2026-05-25".to_string()),
        )?;

        // Find the loan receipt transaction ID (description contains "Loan Received")
        let loan_receipt_id: i64 = conn.query_row(
            "SELECT id FROM transactions WHERE description LIKE '%Loan Received%';",
            [],
            |row| row.get(0),
        )?;

        // Delete the loan receipt. It should delete the entire loan group AND the payment!
        crate::handlers::handle_delete(&conn, loan_receipt_id)?;

        // Verify all transactions in the database (repayments, receipt, and payment) are completely gone!
        let total_tx_count: i64 = conn.query_row("SELECT COUNT(*) FROM transactions;", [], |row| row.get(0))?;
        assert_eq!(total_tx_count, 0);

        Ok(())
    }
}
