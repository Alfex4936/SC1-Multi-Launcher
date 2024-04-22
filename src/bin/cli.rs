use std::{io::Write, path::PathBuf};

use sclauncher::util::{
    admin::{is_admin, run_as_admin},
    game::GameManager,
    reg::{async_registry_search, get_game_path},
};

use clap::Parser;
use winreg::enums::HKEY_LOCAL_MACHINE;

use std::io;

use tokio::time::{sleep, Duration};

#[derive(Parser, Debug)]
#[command(
    name = "SC1 Mutli Loader",
    version = "1.0",
    author = "Seok Won Choi",
    about = "StarCraft 1 Multi launcher"
)]
struct Args {
    /// Performs an asynchronous search in the registry
    #[arg(short, long, action = clap::ArgAction::SetTrue)]
    async_registry_search: bool,

    /// Number of times to launch the game
    #[arg(short, long, default_value_t = 2)]
    num_launches: u32,

    /// 64bits or 32bits
    #[arg(short, long, default_value_t = false)]
    is_64bit: bool,
}

#[tokio::main]
async fn main() {
    if !is_admin() {
        println!("Not running as admin. Attempting to elevate...");
        if run_as_admin() {
            println!("Please restart the application with admin privileges.");
            return; // Exit the current instance
        } else {
            println!("Failed to elevate privileges.");
            return;
        }
    }

    let args = Args::parse();
    let game_manager = GameManager::new();

    // Try to get game path from user input or registry if not found initially
    let game_path = match get_game_path_or_search(&args).await {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    // Launch the game the specified number of times
    launch_game_multiple_times(&game_manager, &game_path, args.num_launches).await;

    println!("Press Enter to kill all games...");
    let mut pause = String::new();
    std::io::stdin().read_line(&mut pause).unwrap();

    game_manager.kill_all_games().await;
}

async fn get_game_path_or_search(args: &Args) -> Result<PathBuf, String> {
    // Try to get the game path directly
    let direct_path = get_game_path(args.is_64bit);

    if args.async_registry_search || direct_path.is_none() {
        println!("Attempting to locate StarCraft.exe...");

        // Perform the registry search if direct path is not found or if async search is requested
        let matches =
            async_registry_search(HKEY_LOCAL_MACHINE, "StarCraft", "InstallLocation").await;
        if matches.is_empty() && direct_path.is_none() {
            // If no matches and no direct path, prompt user for manual input
            return prompt_user_for_path();
        }

        // Use the first found path from registry search if available
        matches.first().map_or_else(
            || direct_path.ok_or_else(|| "StarCraft not found.".to_string()), // Use direct path if no registry matches
            |(_, path)| Ok(PathBuf::from(path)), // Convert the registry path to PathBuf
        )
    } else {
        // If direct path is found, return it
        direct_path.ok_or_else(|| "StarCraft not found.".to_string())
    }
}

fn prompt_user_for_path() -> Result<PathBuf, String> {
    println!("Please enter the full path to StarCraft.exe:");
    let mut path_input = String::new();
    io::stdout().flush().expect("Failed to flush stdout");
    io::stdin()
        .read_line(&mut path_input)
        .map_err(|e| e.to_string())?;

    let trimmed_path = path_input.trim();
    if trimmed_path.is_empty() {
        return Err("No path provided.".to_string());
    }

    let path = PathBuf::from(trimmed_path);
    if !path.exists() {
        return Err("The provided path does not exist.".to_string());
    }
    Ok(path)
}

async fn launch_game_multiple_times(game_manager: &GameManager, path: &PathBuf, num_launches: u32) {
    for i in 0..num_launches {
        println!("Launching StarCraft.exe [{}]", i + 1);
        game_manager.launch_game(path.to_path_buf()).await;
        sleep(Duration::from_secs(1)).await;
    }
}
