use sea_orm::DatabaseConnection;

#[derive(Clone)]
pub struct AppState {
    pub(crate) db: DatabaseConnection,
}

impl AppState {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }
}
