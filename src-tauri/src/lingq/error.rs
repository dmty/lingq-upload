use serde::Serialize;
use specta::Type;
use thiserror::Error;

#[derive(Error, Debug, Serialize, Type, Clone)]
#[serde(tag = "kind", content = "message")]
pub enum LingqError {
    #[error("unauthorized (401)")]
    Unauthorized,
    #[error("not found (404)")]
    NotFound,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("server error: {0}")]
    Server(String),
    #[error("schema drift: {0}")]
    Schema(String),
    #[error("transport: {0}")]
    Transport(String),
    #[error("io: {0}")]
    Io(String),
}
