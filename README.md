```
 _ __ ___   __ _ _ __   __ _  ___
| '_ ` _ \ / _` | '_ \ / _` |/ _ \
| | | | | | (_| | | | | (_| | (_) |
|_| |_| |_|\__,_|_| |_|\__, |\___/
                        __/ |
                       |___/
```

**mango: Your Personal Finance Assistant on the Command Line.**

A powerful, local-first, command-line personal finance tool based on the principles of double-entry accounting. Track your income, expenses, assets, and liabilities with precision and privacy.

---

### Core Features

- **Double-Entry Accounting:** Every transaction is balanced, ensuring financial integrity.
- **CLI-First Interface:** Fast, scriptable, and keeps you in control of your data.
- **Multi-Currency Support:** Track accounts in different currencies with automatic base-currency conversion.
- **Powerful Transaction Projection:** Plan for the future with commands for amortized loans (`loan`) and purchases in installments (`installments`).
- **Asset Revaluation:** Adjust the value of your assets (like investments) to keep your net worth accurate using `account adjust`.
- **Debt Settlement:** Easily pay off monthly credit card statements or other liabilities with the `pay-liability` command.
- **Rich Reporting:** Get insights into your financial health with `balance`, `cashflow`, `expense-report`, and `history-report`.
- **Local & Private:** Your data is stored locally in a simple SQLite database. No servers, no tracking.

### Installation

There are multiple ways to install `mango`. Choose the one that best fits your operating system and preferences.

#### macOS (Homebrew)

If you are on macOS and use [Homebrew](https://brew.sh/), this is the recommended method.

1.  **Add the custom tap:**

    ```sh
    brew tap JulianLobos/homebrew-tap
    ```

2.  **Install `mango`:**
    ```sh
    brew install mango
    ```

#### From Binaries (Windows, Linux, macOS)

You can download a pre-compiled binary for your specific operating system from the [Releases](../../releases) page.

1.  Go to the latest release page.
2.  Download the archive (`.zip` or `.tar.gz`) for your OS.
3.  Extract the archive.
4.  Move the `mango` executable file to a directory included in your system's `PATH` (e.g., `/usr/local/bin` on Linux/macOS, or a custom folder on Windows that you've added to the Path environment variable).

### Quick Start Guide

1.  **Initialize the database:**
    Set up `mango` with your chosen base currency. All reports will be summarized in this currency.

    ```sh
    mango setup --base-currency ARS
    ```

2.  **Add your accounts:**
    You need at least one asset (like a bank account) and one income source.

    ```sh
    # Asset account
    mango account add --name "Banco" --type asset --currency ARS

    # Income source
    mango account add --name "Sueldo" --type income --currency ARS

    # An expense category
    mango account add --name "Comida" --type expense --currency ARS
    ```

3.  **Record your first income:**

    ```sh
    mango income --amount 300000 --account "Banco" --source "Sueldo" --description "Sueldo de Junio"
    ```

4.  **Record an expense:**

    ```sh
    mango expense --amount 15000 --account "Banco" --category "Comida" --description "Compra semanal"
    ```

5.  **Check your balance!**
    ```sh
    mango balance
    ```

### Full Documentation

This README provides a brief overview. For a complete command reference with detailed examples for every feature, please see the **[USER MANUAL](USER_MANUAL.md)**.

### License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
