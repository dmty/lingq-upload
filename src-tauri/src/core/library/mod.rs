pub mod index;
pub mod reconcile;
pub mod trash;

pub use index::{
    derive_status, estimated_total_chapters, load_or_rebuild, rebuild_from_store,
    rebuild_with_status, write_atomic, LibraryEntry, LibraryError, LibraryIndex, LibraryStatus,
    INDEX_FILENAME, INDEX_SCHEMA_V1,
};
pub use reconcile::{candidate_to_id, candidate_to_project, reconcile, ReconcileReport};
pub use trash::{list_trash, purge_project, restore_project, trash_project, TrashEntry};
