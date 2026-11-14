//! Strato compatibile Jellyfin, solo per Infuse (fase 2). Scritto in modo indipendente.

use crate::state::Shared;
use axum::Router;

pub fn router() -> Router<Shared> {
    Router::new()
}
