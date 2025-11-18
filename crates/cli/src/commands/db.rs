use crate::{commands::Result, util::error::UtilError};
use contender_core::db::DbOps;
use contender_sqlite::SqliteDb;
use std::{fs, path::PathBuf};
use tracing::info;

/// Delete the database file
pub async fn drop_db(db_path: &str) -> Result<()> {
    // Check if file exists before attempting to remove
    if fs::metadata(db_path).is_ok() {
        fs::remove_file(db_path)?;
        info!("Database file '{db_path}' has been deleted.");
    } else {
        info!("Database file '{db_path}' does not exist.");
    }
    Ok(())
}

/// Reset the database by dropping it and recreating tables
pub async fn reset_db(db_path: &str) -> Result<()> {
    // Drop the database
    drop_db(db_path).await?;

    // create a new empty file at db_path (to avoid errors when creating tables)
    let db = SqliteDb::from_file(db_path).expect("failed to open contender DB file");

    // Recreate tables
    db.create_tables()?;
    info!("Database has been reset and tables recreated.");
    Ok(())
}

/// Export the database to a file
pub async fn export_db(src_path: &str, target_path: PathBuf) -> Result<()> {
    // Ensure source database exists
    if fs::metadata(src_path).is_err() {
        return Err(UtilError::DBDoesNotExist.into());
    }

    // Copy the database file to the target location
    fs::copy(src_path, &target_path).map_err(UtilError::DBExportFailed)?;
    info!("Database exported to '{}'", target_path.display());
    Ok(())
}

/// Import the database from a file
pub async fn import_db(src_path: PathBuf, target_path: &str) -> Result<()> {
    // Ensure source file exists
    if !src_path.exists() {
        return Err(UtilError::DBDoesNotExist.into());
    }

    // If target exists, create a backup
    if fs::metadata(target_path).is_ok() {
        let backup_path = format!("{target_path}.backup");
        fs::copy(target_path, &backup_path).map_err(UtilError::DBBackupFailed)?;
        info!("Created backup of existing database at '{target_path}.backup'");
    }

    // Copy the source database to the target location
    fs::copy(&src_path, target_path).map_err(UtilError::DBImportFailed)?;
    info!("Database imported from '{}'", src_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Creates a temp directory containing a database file with the given name.
    ///
    /// Returns the temp directory and the full path to the database file.
    fn setup_test_env(name: &str) -> (TempDir, String) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir
            .path()
            .join(format!("test_{name}.db"))
            .to_str()
            .unwrap()
            .to_string();

        (temp_dir, db_path)
    }

    #[tokio::test]
    async fn test_drop_db() {
        let (_temp_dir, db_path) = setup_test_env("drop");

        // Create a dummy file
        fs::write(&db_path, "test data").expect("Failed to write test file");
        assert!(fs::metadata(&db_path).is_ok());

        // Test dropping the database
        drop_db(&db_path).await.expect("Failed to drop database");
        assert!(fs::metadata(&db_path).is_err());
    }

    #[tokio::test]
    async fn test_reset_db() {
        let (_temp_dir, db_path) = setup_test_env("reset");

        // Create a mock database
        fs::write(&db_path, "testing").expect("Failed to write test file");

        // Test resetting the database
        reset_db(&db_path).await.expect("Failed to reset database");
        assert!(fs::metadata(&db_path).is_ok()); // DB file should exist again
    }

    #[tokio::test]
    async fn test_export_import_db() {
        let (temp_dir, db_path) = setup_test_env("export_import");

        // Create a dummy database file
        fs::write(&db_path, "test database content").expect("Failed to write test file");

        // Test export
        let exported_path = temp_dir.path().join("export.db");
        export_db(&db_path, exported_path.clone())
            .await
            .expect("Failed to export database");
        assert!(exported_path.exists());

        // Test import
        fs::remove_file(&db_path).expect("Failed to remove original db");
        import_db(exported_path, &db_path)
            .await
            .expect("Failed to import database");
        assert!(fs::metadata(&db_path).is_ok());

        // Verify content
        let content = fs::read_to_string(&db_path).expect("Failed to read imported db");
        assert_eq!(content, "test database content");
    }
}
