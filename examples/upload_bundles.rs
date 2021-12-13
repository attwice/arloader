use arloader::{commands::*, error::Error, Arweave};
use rand::Rng;
use rayon::prelude::*;
use std::env;
use std::{fs, path::PathBuf, str::FromStr};
use tempdir::TempDir;
use url::Url;

// For smaller sample sizes, you may have to increase this to have the transactions mined.
const REWARD_MULTIPLIER: f32 = 2.0;
const NUM_FILES: usize = 2;
const FILE_SIZE: usize = 10_000_000;
const BUNDLE_SIZE: u64 = 100_000_000;
const BUFFER: usize = 5;

#[tokio::main]
async fn main() -> CommandResult {
    let ar_keypair_path = env::var("AR_KEYPAIR_PATH").ok();
    let sol_keypair_path = env::var("SOL_KEYPAIR_PATH").ok();

    let arweave = if let Some(ar_keypair_path) = ar_keypair_path {
        Arweave::from_keypair_path(
            PathBuf::from(ar_keypair_path),
            Url::from_str("https://arweave.net").unwrap(),
        )
        .await?
    } else {
        if sol_keypair_path.is_none() {
            println!("Example requires either AR_KEYPAIR_PATH or SOL_KEYPAIR_PATH environment variable to be set.");
            return Ok(());
        };
        Arweave::default()
    };

    let ext = "bin";
    let temp_dir = files_setup(FILE_SIZE, NUM_FILES, ext)?;
    let log_dir = temp_dir.path().join("status");
    fs::create_dir(log_dir.clone()).unwrap();
    let glob_str = format!("{}/*.{}", temp_dir.path().display().to_string(), ext);
    let log_dir_str = log_dir.display().to_string();

    if sol_keypair_path.is_none() {
        command_upload_bundles(
            &arweave,
            &glob_str,
            Some(log_dir_str.clone()),
            None,
            BUNDLE_SIZE,
            REWARD_MULTIPLIER,
            None,
            BUFFER,
        )
        .await?;
    } else {
        command_upload_bundles_with_sol(
            &arweave,
            &glob_str,
            Some(log_dir_str.clone()),
            None,
            BUNDLE_SIZE,
            REWARD_MULTIPLIER,
            None,
            BUFFER,
            sol_keypair_path.as_deref().unwrap(),
        )
        .await?;
    }

    command_update_bundle_statuses(&arweave, &log_dir_str, None, 10).await?;
    Ok(())
}

fn files_setup(file_size: usize, num_files: usize, ext: &str) -> Result<TempDir, Error> {
    let mut rng = rand::thread_rng();
    let mut bytes = Vec::with_capacity(file_size);
    (0..file_size).for_each(|_| bytes.push(rng.gen()));

    let temp_dir = TempDir::new("test_files")?;

    let _ = (0..num_files).into_par_iter().for_each(|i| {
        fs::write(
            temp_dir.path().join(format!("{}", i)).with_extension(ext),
            &bytes,
        )
        .unwrap();
    });
    Ok(temp_dir)
}
