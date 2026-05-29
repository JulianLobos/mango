# Mango User Manual

Welcome to the comprehensive user manual for `mango`. This document provides a detailed reference for every command and feature available in the application.

## Table of Contents
1.  [Core Concepts](#core-concepts)
    - [Double-Entry Accounting](#double-entry-accounting)
    - [Account Types](#account-types)
    - [Multi-Currency](#multi-currency)
2.  [Command Reference](#command-reference)
    - [`setup`](#setup)
    - [`account`](#account)
    - [`income`](#income)
    - [`expense`](#expense)
    - [`transfer`](#transfer)
    - [`loan`](#loan)
    - [`installments`](#installments)
    - [`pay-liability`](#pay-liability)
    - [`list`](#list)
    - [`find`](#find)
    - [`edit`](#edit)
    - [`delete`](#delete)
    - [`export`](#export)
    - [`balance`](#balance)
    - [`cashflow`](#cashflow)
    - [`expense-report`](#expense-report)
    - [`history-report`](#history-report)
    - [`budget`](#budget)

---

## Core Concepts

### Double-Entry Accounting
`mango` is built on the principle of double-entry accounting. This means every transaction affects at least two accounts and the total sum of the transaction is always zero. For example, an expense transaction moves money *from* an `asset` account (like a bank) *to* an `expense` account (like "Food"). This ensures your financial records are always balanced and accurate.

### Account Types
There are five types of accounts in `mango`:
-   **`asset`**: What you own. e.g., Bank accounts, cash, investments.
-   **`liability`**: What you owe. e.g., Credit cards, loans.
-   **`income`**: Where your money comes from. e.g., Salary, freelance work.
-   **`expense`**: Where your money goes. e.g., Food, rent, transport.
-   **`equity`**: Represents the net worth or value adjustments. Used for special transactions like asset revaluation.

### Multi-Currency
`mango` supports tracking accounts in multiple currencies. It requires a single **base currency** to be set during `setup`. All reports summarize totals in this base currency. When you perform a transaction involving a non-base currency, you must provide an `--exchange-rate` so `mango` can correctly calculate its value in your base currency.

---

## Command Reference

### `setup`
Initializes the database and sets the base currency. This command must be run first.

**Usage:**
```sh
mango setup --base-currency <CURRENCY_CODE>
```
**Arguments:**
-   `--base-currency <CODE>`: (Required) The 3-letter code for your base currency (e.g., `USD`, `ARS`, `EUR`).

**Example:**
```sh
mango setup --base-currency ARS
```

### `account`
Manages your accounts.

#### `account add`
Adds a new account.

**Usage:**
```sh
mango account add --name <NAME> --type <TYPE> --currency <CURRENCY>
```
**Arguments:**
-   `--name <NAME>`: The name of the account (e.g., "Banco Galicia", "Inversiones USD").
-   `--type <TYPE>`: The account type. Must be one of `asset`, `liability`, `income`, `expense`, `equity`.
-   `--currency <CURRENCY>`: The 3-letter currency code for this account.

**Example:**
```sh
mango account add --name "Inversiones" --type asset --currency USD
```

#### `account list`
Lists all existing accounts.

**Usage:**
```sh
mango account list
```

#### `account update`
Changes the name or currency of an existing account.

**Usage:**
```sh
mango account update --old-name <OLD_NAME> [OPTIONS]
```
**Arguments:**
-   `--old-name <OLD_NAME>`: The current name of the account you want to change.
-   `--new-name <NEW_NAME>`: (Optional) The new name for the account.
-   `--currency <CURRENCY>`: (Optional) The new currency for the account.

#### `account delete`
Deletes an account. This only works if the account has no transactions associated with it.

**Usage:**
```sh
mango account delete --name <NAME>
```

#### `account adjust`
Adjusts the balance of an **asset** account to a new, specified value. This is useful for revaluing assets like investments whose market value has changed.

**Usage:**
```sh
mango account adjust --name <NAME> --new-balance <AMOUNT> [OPTIONS]
```
**Arguments:**
-   `--name <NAME>`: The name of the asset account to adjust.
-   `--new-balance <AMOUNT>`: The new total balance for the account in its native currency.
-   `--description <DESC>`: (Optional) A description for the adjustment transaction.
-   `--exchange-rate <RATE>`: (Optional) **Required** if the account is not in your base currency. This is the current exchange rate used to value the gain/loss in your base currency (e.g., `1500` if 1 USD = 1500 ARS).

**How it works:**
This command calculates the difference between the old and new balance and creates a transaction that debits/credits the asset account. The balancing entry is made against a special `equity` account named "Equity Adjustments", so it doesn't affect your income/expense reports.

**Example:**
```sh
# Adjust an ARS account
mango account adjust --name "Acciones ARS" --new-balance 150000

# Adjust a USD account (base currency is ARS)
mango account adjust --name "Inversiones" --new-balance 95 --exchange-rate 1500
```

### `income`
Records an income transaction.

**Usage:**
```sh
mango income --amount <AMOUNT> --account <ASSET_ACCOUNT> --source <INCOME_ACCOUNT> --description <DESC>
```
**Arguments:**
-   `--amount <AMOUNT>`: The amount of income received.
-   `--account <ASSET>`: The asset account where the money was deposited (e.g., "Banco").
-   `--source <INCOME>`: The income account representing the source (e.g., "Sueldo").
-   `--description <DESC>`: A description of the transaction.
-   `--date <YYYY-MM-DD>`: (Optional) The date of the transaction. Defaults to today.

### `expense`
Records an expense transaction.

**Usage:**
```sh
mango expense --amount <AMOUNT> --account <ASSET_ACCOUNT> --category <EXPENSE_ACCOUNT> --description <DESC>
```
**Arguments:**
-   `--amount <AMOUNT>`: The amount of the expense.
-   `--account <ASSET>`: The asset account from which the money was paid (e.g., "Banco", "Efectivo").
-   `--category <EXPENSE>`: The expense account representing the category (e.g., "Comida", "Transporte").
-   `--description <DESC>`: A description of the transaction.
-   `--date <YYYY-MM-DD>`: (Optional) The date of the transaction. Defaults to today.

### `transfer`
Records a transfer of funds between two accounts.

**Usage:**
```sh
mango transfer --amount <AMOUNT> --from <FROM_ACCOUNT> --to <TO_ACCOUNT> --description <DESC>
```
**Arguments:**
-   `--amount <AMOUNT>`: The amount to transfer, in the currency of the `--from` account.
-   `--from <ACCOUNT>`: The account to transfer from.
-   `--to <ACCOUNT>`: The account to transfer to.
-   `--description <DESC>`: A description of the transaction.
-   `--date <YYYY-MM-DD>`: (Optional) The date of the transaction.
-   `--exchange-rate <RATE>`: (Optional) **Required** for cross-currency transfers. The rate to convert from the `from` currency to the `to` currency.
-   `--cost-amount <AMOUNT>`: (Optional) Alternative to `--exchange-rate`. Specify the exact final amount in the `to` account's currency. `mango` will calculate the effective rate.

**Example (Cross-currency):**
```sh
# Transfer 100,000 ARS from 'banco' to 'inversiones' (USD)
mango transfer --amount 100000 --from "banco" --to "inversiones" --exchange-rate 1450
```

### `loan`
Creates a loan and projects all future repayments, including principal and interest.

**Usage:**
```sh
mango loan --principal <P_AMT> --total-amount <T_AMT> --installments <N> --bank-account <ASSET> --loan-account <LIABILITY> --interest-account <EXPENSE> --description <DESC>
```
**Arguments:**
-   `--principal <P_AMT>`: The initial amount of money received.
-   `--total-amount <T_AMT>`: The full amount that will be paid back over the life of the loan.
-   `--installments <N>`: The number of monthly repayments.
-   `--bank-account <ASSET>`: The asset account where you receive the principal and from which repayments are made.
-   `--loan-account <LIABILITY>`: The liability account for the loan.
-   `--interest-account <EXPENSE>`: The expense account to track interest paid.
-   `--description <DESC>`: A description for the loan.
-   `--date <YYYY-MM-DD>`: (Optional) The date the loan was received. Repayments are projected monthly from this date.

### `installments`
Records a financed purchase (e.g., with a credit card) and projects all future installments.

**Usage:**
```sh
mango installments --cash-amount <C_AMT> --total-amount <T_AMT> --installments <N> --account <LIABILITY> --category <EXPENSE> --interest-account <EXPENSE> --description <DESC>
```
**Arguments:**
-   `--cash-amount <C_AMT>`: The original price of the item if it were bought in cash.
-   `--total-amount <T_AMT>`: The total amount to be paid, including interest.
-   `--installments <N>`: The number of installments.
-   `--account <LIABILITY>`: The liability account for the debt (e.g., "Tarjeta de Crédito").
-   `--category <EXPENSE>`: The expense category for the purchase (e.g., "Electrónica").
-   `--interest-account <EXPENSE>`: The expense account to track financing costs.
-   `--description <DESC>`: A description for the purchase.
-   `--date <YYYY-MM-DD>`: (Optional) The date of the first installment.

### `pay-liability`
Settles a month's debt for a liability account, typically used to record a credit card payment.

**Usage:**
```sh
mango pay-liability --account <LIABILITY> --from <ASSET> --month <M> --year <Y>
```
**Arguments:**
-   `--account <LIABILITY>`: The liability account to pay off (e.g., "Tarjeta de Crédito").
-   `--from <ASSET>`: The asset account to pay from (e.g., "Banco").
-   `--month <M>`: The month (1-12) of the debt you want to settle.
-   `--year <Y>`: The year of the debt you want to settle.
-   `--date <YYYY-MM-DD>`: (Optional) The date the payment was made. Defaults to today.

**How it works:**
This command sums up all negative movements (new debts) in the specified liability account for the given month/year and creates a single transfer from your asset account to cancel them out.

### `list`
Lists recent transactions or filters them by a time period.

**Usage:**
```sh
mango list [OPTIONS]
```
**Arguments:**
-   `--limit <N>`: (Default: 20) The number of recent transactions to show. Ignored if date filters are used.
-   `--month <M>`: (Optional) Filter transactions by a specific month (1-12). Defaults to the current year if `--year` is not provided.
-   `--year <Y>`: (Optional) Filter transactions by a specific year.

### `find`
Finds transactions by searching their description.

**Usage:**
```sh
mango find <QUERY>
```

### `edit`
Edits the date, description, or amount of a simple transaction.

**Usage:**
```sh
mango edit <ID> [OPTIONS]
```
**Arguments:**
-   `<ID>`: The ID of the transaction to edit.
-   `--date <YYYY-MM-DD>`: (Optional) The new date.
-   `--description <DESC>`: (Optional) The new description.
-   `--amount <AMOUNT>`: (Optional) The new amount. **Note:** Only works on simple, two-entry transactions.

### `delete`
Deletes a transaction and all its associated entries.

**Usage:**
```sh
mango delete <ID>
```

### `export`
Exports all transaction entries to a CSV file. The file includes a UTF-8 BOM for better compatibility with Excel.

**Usage:**
```sh
mango export --file <PATH_TO_FILE>
```
**Example:**
```sh
mango export --file "my-finances.csv"
```

### `balance`
Displays a balance sheet with the current balance of all asset and liability accounts, and calculates your net worth.

**Usage:**
```sh
mango balance [OPTIONS]
```
**Arguments:**
-   `--to <YYYY-MM-DD>`: (Optional) Show balance up to a specific date.
-   `--month <M>` / `--year <Y>`: (Optional) Shortcuts to show the balance at the end of a specific month/year.
-   `--live-rate <RATES>`: (Optional) Provide current exchange rates to re-evaluate non-base currency assets at market value. Format: `"USD:1500,EUR:1600"`.

### `cashflow`
Displays an income statement for a given period, showing total income, total expenses, and net savings.

**Usage:**
```sh
mango cashflow [OPTIONS]
```

### `expense-report`
Shows a detailed breakdown of expenses by category for a given period, including percentages and budget status.

**Usage:**
```sh
mango expense-report [OPTIONS]
```

### `history-report`
Shows a month-by-month evolution of income, expenses, and savings.

**Usage:**
```sh
mango history-report --months <N>
```
**Arguments:**
-   `--months <N>`: (Default: 12) The number of past months to show.

### `budget`
Manages monthly spending budgets for expense categories.

#### `budget set`
Sets or updates the monthly budget for a category.
```sh
mango budget set --category <EXPENSE_ACCOUNT> --limit <AMOUNT>
```

#### `budget list`
Lists all currently set budgets.
```sh
mango budget list
```

#### `budget delete`
Removes the budget for a category.
```sh
mango budget delete --category <EXPENSE_ACCOUNT>
```
