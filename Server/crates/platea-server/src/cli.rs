//! Comandi da terminale, eseguiti nel container accanto al server in funzione:
//!
//!   docker compose exec server platea reset-password <utente> [nuova-password]
//!
//! Senza password ne genera una casuale e la stampa. Scollega tutti i dispositivi dell'utente
//! (entro un minuto anche quelli in cache nel server) e riattiva l'account se era disattivato.

use crate::boot::BootConfig;
use anyhow::{bail, Context};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use sqlx::ConnectOptions;
use std::time::Duration;

/// `Some(codice di uscita)` se gli argomenti sono un comando da terminale.
pub fn dispatch(boot: &BootConfig) -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first()?.as_str();
    if cmd != "reset-password" {
        return None;
    }
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
    let res = rt.block_on(reset_password(boot, args.get(1).map(String::as_str), args.get(2).cloned()));
    Some(match res {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("Errore: {e:#}");
            1
        }
    })
}

/// Password casuale leggibile: 16 caratteri senza quelli che si confondono (0/O, 1/l/I).
fn random_password() -> String {
    use rand::Rng;
    const ALPHABET: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut rng = rand::thread_rng();
    (0..16).map(|_| ALPHABET[rng.gen_range(0..ALPHABET.len())] as char).collect()
}

async fn reset_password(boot: &BootConfig, username: Option<&str>, password: Option<String>) -> anyhow::Result<()> {
    let Some(username) = username.map(str::trim).filter(|u| !u.is_empty()) else {
        bail!("uso: platea reset-password <utente> [nuova-password]");
    };
    let path = boot.db_path();
    if !path.exists() {
        bail!("database non trovato in {}: il server non è mai partito?", path.display());
    }
    // Stesso database del server in funzione: WAL e attesa sui lock, nessuna migrazione.
    let mut conn = SqliteConnectOptions::new()
        .filename(&path)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10))
        .foreign_keys(true)
        .connect()
        .await
        .context("apertura del database")?;

    let user: Option<(i64, String)> =
        sqlx::query_as("SELECT id, username FROM users WHERE username_lower = ?").bind(username.to_lowercase()).fetch_optional(&mut conn).await?;
    let Some((id, name)) = user else {
        let all: Vec<String> = sqlx::query_scalar("SELECT username FROM users ORDER BY id").fetch_all(&mut conn).await?;
        bail!("l'utente \"{username}\" non esiste. Utenti: {}", if all.is_empty() { "nessuno".into() } else { all.join(", ") });
    };

    let generated = password.is_none();
    let password = password.unwrap_or_else(random_password);
    if password.chars().count() < 8 {
        bail!("la password deve avere almeno 8 caratteri");
    }
    let hash = crate::auth::hash_password(password.clone()).await?;

    let mut tx = sqlx::Connection::begin(&mut conn).await?;
    sqlx::query("UPDATE users SET password_hash = ?, disabled = 0 WHERE id = ?").bind(&hash).bind(id).execute(&mut *tx).await?;
    let revoked = sqlx::query("DELETE FROM tokens WHERE user_id = ?").bind(id).execute(&mut *tx).await?.rows_affected();
    sqlx::query("INSERT INTO activity_log (at, user_id, kind, message, ip) VALUES (?, ?, 'password_reset', ?, NULL)")
        .bind(crate::util::now_ms())
        .bind(id)
        .bind(format!("Password di {name} reimpostata da terminale"))
        .execute(&mut *tx)
        .await
        .ok();
    tx.commit().await?;

    println!("Password di \"{name}\" reimpostata.");
    if generated {
        println!("Nuova password: {password}");
    }
    println!("Dispositivi scollegati: {revoked}. L'account è attivo.");
    Ok(())
}
