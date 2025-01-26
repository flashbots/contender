use contender_core::{db::DbOps, error::ContenderError, Result};
use std::{fs, path::PathBuf};

fn get_default_db_path() -> String {
    format!(
        "{}/.contender/contender.db",
        std::env::var("HOME").expect("$HOME not found in environment")
    )
}

/// Delete the database file
pub async fn drop_db() -> Result<()> {
    // Get the database file path from environment or use default
    let db_path = std::env::var("CONTENDER_DB_PATH").unwrap_or_else(|_| get_default_db_path());

    // Check if file exists before attempting to remove
    if fs::metadata(&db_path).is_ok() {
        fs::remove_file(&db_path).map_err(|e| {
            ContenderError::DbError("Failed to delete database file", Some(e.to_string()))
        })?;
        println!("Database file '{}' has been deleted.", db_path);
    } else {
        println!("Database file '{}' does not exist.", db_path);
    }
    Ok(())
}

/// Reset the database by dropping it and recreating tables
pub async fn reset_db(db: &impl DbOps) -> Result<()> {
    // Drop the database
    drop_db().await?;

    // Recreate tables
    db.create_tables()?;
    println!("Database has been reset and tables recreated.");
    Ok(())
}

/// Export the database to a file
pub async fn export_db(out_path: PathBuf) -> Result<()> {
    // Get the source database path
    let src_path = std::env::var("CONTENDER_DB_PATH").unwrap_or_else(|_| get_default_db_path());

    // Ensure source database exists
    if !fs::metadata(&src_path).is_ok() {
        return Err(ContenderError::DbError(
            "Source database file does not exist",
            None,
        ));
    }

    // Copy the database file to the target location
    fs::copy(&src_path, &out_path)
        .map_err(|e| ContenderError::DbError("Failed to export database", Some(e.to_string())))?;
    println!("Database exported to '{}'", out_path.display());
    Ok(())
}

/// Import the database from a file
pub async fn import_db(src_path: PathBuf) -> Result<()> {
    // Ensure source file exists
    if !src_path.exists() {
        return Err(ContenderError::DbError(
            "Source database file does not exist",
            None,
        ));
    }

    // Get the target database path
    let target_path = std::env::var("CONTENDER_DB_PATH").unwrap_or_else(|_| get_default_db_path());

    // If target exists, create a backup
    if fs::metadata(&target_path).is_ok() {
        let backup_path = format!("{}.backup", target_path);
        fs::copy(&target_path, &backup_path)
            .map_err(|e| ContenderError::DbError("Failed to create backup", Some(e.to_string())))?;
        println!(
            "Created backup of existing database at '{}.backup'",
            target_path
        );
    }

    // Copy the source database to the target location
    fs::copy(&src_path, &target_path)
        .map_err(|e| ContenderError::DbError("Failed to import database", Some(e.to_string())))?;
    println!("Database imported from '{}'", src_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use contender_core::db::MockDb;
    use std::env;
    use tempfile::TempDir;

    // NOTE: These tests need to be ran sequentially i.e. `--test-threads=1`
    // This is because the tests are using the same database file.
    fn setup_test_env() -> (TempDir, String) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir
            .path()
            .join("test.db")
            .to_str()
            .unwrap()
            .to_string();
        env::set_var("CONTENDER_DB_PATH", &db_path);
        (temp_dir, db_path)
    }

    #[tokio::test]
    async fn test_drop_db() {
        let (_temp_dir, db_path) = setup_test_env();

        // Create a dummy file
        fs::write(&db_path, "test data").expect("Failed to write test file");
        assert!(fs::metadata(&db_path).is_ok());

        // Test dropping the database
        drop_db().await.expect("Failed to drop database");
        assert!(fs::metadata(&db_path).is_err());
    }

    #[tokio::test]
    async fn test_reset_db() {
        let (_temp_dir, db_path) = setup_test_env();

        // Create a mock database
        let mock_db = MockDb;

        // Test resetting the database
        reset_db(&mock_db).await.expect("Failed to reset database");
        assert!(fs::metadata(&db_path).is_err()); // Should be dropped
    }

    #[tokio::test]
    async fn test_export_import_db() {
        let (temp_dir, db_path) = setup_test_env();

        // Create a dummy database file
        fs::write(&db_path, "test database content").expect("Failed to write test file");

        // Test export
        let export_path = temp_dir.path().join("export.db");
        export_db(export_path.clone())
            .await
            .expect("Failed to export database");
        assert!(export_path.exists());

        // Test import
        fs::remove_file(&db_path).expect("Failed to remove original db");
        import_db(export_path)
            .await
            .expect("Failed to import database");
        assert!(fs::metadata(&db_path).is_ok());

        // Verify content
        let content = fs::read_to_string(&db_path).expect("Failed to read imported db");
        assert_eq!(content, "test database content");
    }
}
