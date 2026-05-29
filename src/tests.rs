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
}
