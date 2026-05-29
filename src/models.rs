// src/models.rs
pub struct EntryInput {
    pub account_name: String,
    pub amount_native: i64,
    pub exchange_rate: Option<f64>,
}

pub struct TransactionInput {
    pub date: String,
    pub description: String,
    pub installment_current: Option<i64>,
    pub installment_total: Option<i64>,
    pub parent_group_id: Option<String>,
    pub entries: Vec<EntryInput>,
}
