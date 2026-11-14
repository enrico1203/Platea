//! Classificazioni per età. Le schede arrivano con sigle di paesi diversi (T, VM14, PG-13, TV-MA, FSK 16, 12A…):
//! ognuna si traduce in un'età minima, così il limite di un utente vale qualunque sia la fonte.
//! L'età si salva in `items.rating_age`, ricalcolata quando la classificazione cambia (un trigger la azzera).

use crate::state::Shared;

/// Età minima di una classificazione; `None` se la sigla è sconosciuta o vuota.
pub fn age(rating: &str) -> Option<u8> {
    let r = rating.trim();
    if r.is_empty() {
        return None;
    }
    // "IT:VM14", "Rated R", "FSK 16"…
    let r = r.rsplit(':').next().unwrap_or(r).trim();
    let up = r.to_ascii_uppercase();
    let up = up.trim_start_matches("RATED ").trim();
    let known: &[(&str, u8)] = &[
        // Italia
        ("T", 0),
        ("VM6", 6),
        ("VM12", 12),
        ("VM14", 14),
        ("VM18", 18),
        // Stati Uniti, cinema e TV
        ("G", 0),
        ("PG", 10),
        ("PG-13", 13),
        ("R", 17),
        ("NC-17", 18),
        ("NR", 0),
        ("TV-Y", 0),
        ("TV-Y7", 7),
        ("TV-G", 0),
        ("TV-PG", 10),
        ("TV-14", 14),
        ("TV-MA", 17),
        // Regno Unito
        ("U", 0),
        ("12A", 12),
        // Altri comuni
        ("ALL", 0),
        ("KIDS", 0),
        ("AL", 0),
        ("TP", 0),
        ("E", 0),
        ("X", 18),
        ("XXX", 18),
    ];
    if let Some((_, a)) = known.iter().find(|(k, _)| *k == up) {
        return Some(*a);
    }
    // "14+", "FSK 16", "BBFC 15", "-12", "16": il primo numero plausibile.
    let digits: String = up.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<u8>().ok().filter(|a| *a <= 21)
}

/// Calcola l'età per le classificazioni che non ce l'hanno ancora. Le sigle distinte sono poche: costa nulla.
pub async fn sync(state: &Shared) -> anyhow::Result<u64> {
    let pending: Vec<String> = sqlx::query_scalar("SELECT DISTINCT official_rating FROM items WHERE official_rating IS NOT NULL AND rating_age IS NULL")
        .fetch_all(&state.db.read)
        .await?;
    let mut n = 0;
    for r in pending {
        // Sconosciuta: -1, così non si ricalcola ogni minuto e il filtro la tratta come non classificata.
        let a = age(&r).map(|a| a as i64).unwrap_or(-1);
        n += sqlx::query("UPDATE items SET rating_age = ? WHERE official_rating = ? AND rating_age IS NULL")
            .bind(a)
            .bind(&r)
            .execute(&state.db.write)
            .await?
            .rows_affected();
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::age;

    #[test]
    fn ratings_from_many_countries() {
        assert_eq!(age("T"), Some(0));
        assert_eq!(age("VM14"), Some(14));
        assert_eq!(age("IT:VM18"), Some(18));
        assert_eq!(age("PG-13"), Some(13));
        assert_eq!(age("Rated R"), Some(17));
        assert_eq!(age("TV-MA"), Some(17));
        assert_eq!(age("FSK 16"), Some(16));
        assert_eq!(age("14+"), Some(14));
        assert_eq!(age("12A"), Some(12));
        assert_eq!(age("6"), Some(6));
        assert_eq!(age("Kids"), Some(0));
        assert_eq!(age("Kids+13"), Some(13));
        assert_eq!(age(""), None);
        assert_eq!(age("Sconosciuto"), None);
        assert_eq!(age("1994"), None);
    }
}
