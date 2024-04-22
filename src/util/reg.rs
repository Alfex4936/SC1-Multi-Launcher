use std::path::PathBuf;

use regex::Regex;
use tokio::sync::mpsc;
use winreg::{enums::*, RegKey, HKEY};

pub fn get_game_path(is_64bits: bool) -> Option<PathBuf> {
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut subkey_path = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\StarCraft";

    // Try to open the primary registry key
    let subkey = hklm
        .open_subkey_with_flags(subkey_path, KEY_READ)
        .or_else(|_| {
            // If the primary key fails, try the secondary path for 32-bit applications on 64-bit OS
            subkey_path =
                r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\StarCraft";
            hklm.open_subkey_with_flags(subkey_path, KEY_READ)
        })
        .ok()?;

    // Attempt to read the installation location
    let install_location: String = subkey.get_value("InstallLocation").ok()?;
    let exe_subfolder = if is_64bits { "x86_64" } else { "x86" };
    let game_path =
        PathBuf::from(install_location).join(format!("{}\\StarCraft.exe", exe_subfolder));

    // Check if the constructed path exists
    if game_path.exists() {
        Some(game_path)
    } else {
        None
    }
}

pub async fn async_registry_search(
    root_key: HKEY,
    search_term: &str,
    value_name: &str,
) -> Vec<(String, String)> {
    let (tx, mut rx) = mpsc::channel(100);

    let search_term = search_term.to_lowercase(); // Use lowercase for case insensitive checks
    let value_name = value_name.to_string();

    // Compile regex once
    let search_regex = Regex::new(&format!(r"(?i)^{}$", regex::escape(&search_term))).unwrap();
    let initial_path = "HKEY_LOCAL_MACHINE".to_string(); // Starting path

    tokio::spawn(async move {
        let reg_key = RegKey::predef(root_key);
        if let Err(e) =
            recursive_search(reg_key, initial_path, &value_name, &search_regex, tx).await
        {
            eprintln!("Error in recursive search: {}", e);
        }
    });

    let mut results = Vec::new();
    while let Some(result) = rx.recv().await {
        results.push(result);
    }

    results
}

async fn recursive_search(
    root_key: RegKey,
    root_path: String,
    value_name: &str,
    search_regex: &Regex,
    tx: mpsc::Sender<(String, String)>,
) -> tokio::io::Result<()> {
    let mut stack = vec![(root_key, root_path)];

    while let Some((reg_key, path)) = stack.pop() {
        for subkey_name in reg_key.enum_keys() {
            match subkey_name {
                Ok(subkey_name) => {
                    let full_subkey_path = format!("{}\\{}", path, subkey_name); // Build the full path

                    // Case-insensitive check
                    match reg_key.open_subkey(&subkey_name) {
                        Ok(subkey) => {
                            if search_regex.is_match(&subkey_name) {
                                if let Ok(value) = subkey.get_value::<String, _>(value_name) {
                                    tx.send((full_subkey_path.clone(), value)).await.map_err(
                                        |e| {
                                            tokio::io::Error::new(
                                                tokio::io::ErrorKind::Other,
                                                e.to_string(),
                                            )
                                        },
                                    )?;
                                }
                            }
                            stack.push((subkey, full_subkey_path));
                        }
                        Err(_) => continue,
                    }
                }
                Err(_) => continue,
            }
        }
    }
    Ok(())
}
